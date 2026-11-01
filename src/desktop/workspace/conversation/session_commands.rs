use super::*;
use crate::infrastructure::filesystem::project_paths::project_path;

impl Workspace {
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            self.runtime.error =
                Some("当前窗口为只读状态，无法发送或保存。请先解决数据目录问题。".into());
            cx.notify();
            return;
        }
        if self.creating_session()
            || self.deleting_active_session()
            || self.runtime.preparing.is_some()
            || self.runtime.stopping
            || self.runtime.pending_send.is_some()
            || self.runtime.steering_request.is_some()
            || self.importing_attachments()
        {
            return;
        }
        let text = self
            .runtime
            .composer_state
            .input
            .read(cx)
            .value()
            .trim_end()
            .to_string();
        if self.route_settings_command(&text, window, cx) {
            return;
        }
        let Some(s) = self.session() else {
            self.open_project(window, cx);
            return;
        };
        if text.trim().is_empty()
            && !s.comments.iter().any(|c| !c.sent)
            && s.draft_attachments.is_empty()
        {
            return;
        }
        let prompt = s.prompt(&text, self.runtime.follow_up.as_deref());
        let attachments = s.draft_attachments.clone();
        let comment_ids = s
            .comments
            .iter()
            .filter(|comment| !comment.sent || Some(&comment.id) == self.runtime.follow_up.as_ref())
            .map(|comment| comment.id.clone())
            .collect();
        if !self.demo && !self.runtime.connected {
            self.runtime.error = Some("OMP 尚未连接，请稍后发送或在设置中重新连接".into());
            cx.notify();
            return;
        }
        if attachments
            .iter()
            .any(|attachment| attachment.mime.is_some())
            && !self.demo
            && !self.image_input_available()
        {
            self.runtime.error =
                Some("当前模型及已配置模型均不支持图片；附件已保留，请先配置视觉模型。".into());
            cx.notify();
            return;
        }
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(error) => {
                self.runtime.error = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        let session = s.id.clone();
        let (tx, rx) = mpsc::channel();
        let importing = attachments.clone();
        self.runtime.preparing = Some(super::submission::PreparingSubmission {
            comment_ids,
            text,
            attachments,
            follow_up: self.runtime.follow_up.clone(),
            result: rx,
        });
        self.runtime.status = "正在准备输入与附件…".into();
        std::thread::spawn(move || {
            let result = host
                .prepare_input(&session, &prompt, &importing)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(result);
        });
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn submit_prepared(
        &mut self,
        prepared: super::submission::PreparingSubmission,
        prompt: String,
        images: Vec<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = prepared.text;
        let attachments = prepared.attachments;
        let comment_ids = prepared.comment_ids;
        let Some(s) = self.session() else {
            return;
        };
        if !images.is_empty() && !self.demo && !self.image_input_available() {
            self.runtime.error = Some(
                "当前模型及已配置模型均不支持图片；请先在模型设置中配置视觉模型。附件已保留。"
                    .into(),
            );
            cx.notify();
            return;
        }
        if self.demo {
            let follow = prepared.follow_up;
            if let Some(s) = self.session_mut() {
                s.submit_selected(text, follow, &comment_ids);
                if let Some(m) = s.messages.last_mut() {
                    m.attachments = std::mem::take(&mut s.draft_attachments);
                }
                s.draft.clear();
            }
            self.runtime
                .composer_state
                .input
                .update(cx, |i, cx| i.set_value("", window, cx));
            self.runtime.status = "演示：已保留评论记录，未调用模型".into();
            self.sync_texts(cx);
            self.persist();
            cx.notify();
            return;
        }
        if !self.runtime.connected {
            self.runtime.error = Some("OMP 尚未连接，请稍后发送或在设置中重新连接".into());
            cx.notify();
            return;
        }
        if s.environment_id() == "local"
            && s.omp_session
                .as_deref()
                .is_some_and(|p| !std::path::Path::new(p).is_file())
        {
            self.runtime.engine = None;
            self.runtime.connected = false;
            self.runtime.error = Some("原生会话已移除，正在同步列表".into());
            self.scan_sessions();
            cx.notify();
            return;
        }
        let request_id = id();
        if !self.runtime.running {
            self.runtime.in_flight_comments.clear();
            self.runtime.submitted_inputs.clear();
            self.runtime.interrupted_run = false;
            self.runtime.run_message_start = self.session().map(|s| s.messages.len()).unwrap_or(0);
        }
        if let Some(e) = &self.runtime.engine {
            let mut frame = json!({"type":"prompt","id":request_id,"message":prompt});
            if !images.is_empty() {
                frame["images"] = json!(images);
            }
            if self.runtime.running {
                frame["streamingBehavior"] = json!("followUp");
            }
            match e.send(frame) {
                Ok(()) => {
                    if self.runtime.follow_up == prepared.follow_up {
                        self.runtime.follow_up = None;
                    }
                    self.runtime.pending_send = Some(PendingSubmission {
                        comment_ids,
                        id: request_id,
                        text,
                        follow_up: prepared.follow_up,
                        was_running: self.runtime.running,
                        wire_text: prompt,
                        delivered: false,
                        attachments,
                    });
                    if let Some(s) = self.session_mut() {
                        s.draft.clear();
                        s.draft_attachments.clear();
                    }
                    self.runtime
                        .composer_state
                        .input
                        .update(cx, |i, cx| i.set_value("", window, cx));
                    self.runtime.running = true;
                    self.runtime.status = "正在提交指令…".into();
                }
                Err(e) => self.runtime.error = Some(e.to_string()),
            }
        }
        cx.notify();
    }
    pub fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            return;
        }
        use gpui_kit::component::{
            button::{Button, ButtonVariants},
            dialog::{DialogAction, DialogClose, DialogFooter},
        };
        use omp_core::environment::{HostPath, ProjectLocation};
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(error) => {
                self.runtime.error = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        let recent = self
            .environment_projects()
            .into_iter()
            .rev()
            .map(|project| project.path)
            .collect();
        let environment = self.environment().clone();
        let picker = cx.new(|cx| {
            crate::desktop::project_picker::ProjectPicker::new(
                std::sync::Arc::new(host),
                recent,
                window,
                cx,
            )
        });
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let selected = picker.clone();
            let weak = weak.clone();
            let environment = environment.clone();
            dialog
                .title("选择 OMP 工作目录")
                .child(picker.clone())
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("取消")))
                        .child(
                            DialogAction::new().child(
                                Button::new("open-project-directory")
                                    .primary()
                                    .label("打开目录"),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let Some(path) = selected.read(cx).selected(cx) else {
                        return false;
                    };
                    let _ = weak.update(cx, |this, cx| {
                        if this.db.active_environment != environment.id {
                            return;
                        }
                        match path {
                            HostPath::Local(path) => {
                                if !this.db.projects.contains(&path) {
                                    this.db.projects.push(path);
                                }
                            }
                            path @ HostPath::Posix(_) => {
                                let project = ProjectLocation {
                                    environment: environment.id.clone(),
                                    profile: environment.profile.clone(),
                                    path,
                                };
                                if !this.db.remote_projects.contains(&project) {
                                    this.db.remote_projects.push(project);
                                }
                            }
                        }
                        crate::infrastructure::filesystem::project_paths::normalize_workspace(
                            &mut this.db,
                        );
                        this.persist();
                        this.scan_sessions();
                        cx.notify();
                    });
                    true
                })
        });
    }
    pub fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.db.sessions.get(ix).is_none_or(|s| s.deleted) {
            return;
        }
        let target = self.db.sessions[ix].id.clone();
        if self.document_state.prompt.is_some() && !self.document_state.discard_permit {
            return;
        }
        if self.active == Some(ix) {
            self.settings_view = None;
            self.activate_document(None);
            cx.notify();
            return;
        }
        if !self.guard_documents(documents::LeaveAction::Activate(target.clone()), window, cx) {
            return;
        }
        self.persist();
        let previous = self.session().map(|s| s.id.clone());
        let cleanup_error = self.discard_empty_session_on_leave(&target);
        let ix = self
            .db
            .sessions
            .iter()
            .position(|s| s.id == target)
            .unwrap();
        self.active = previous.and_then(|id| self.db.sessions.iter().position(|s| s.id == id));
        self.switch_session_runtime(&target, window, cx);
        self.active = Some(ix);
        self.db.active = Some(target);
        self.search_state.hits.clear();
        self.search_state.rendered_text.clear();
        self.search_state.index = 0;
        self.clear_file_preview();
        self.clear_documents();
        self.settings_view = None;
        self.comment_editor = None;
        self.editing_comment = None;
        self.document_state.files.clear();
        self.document_state.changes.clear();
        self.runtime.todos = self.db.sessions[ix].todos.clone();
        self.runtime.agents = self.db.sessions[ix].agents.clone();
        self.sync_texts(cx);
        if self.runtime.engine.is_none() {
            self.connect();
        }
        if let Some(error) = cleanup_error {
            self.runtime.error = Some(error);
        }
        self.refresh_files(cx);
        self.persist();
        cx.notify();
    }
    pub fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            return;
        }
        if let Some(s) = self.session() {
            self.new_session_at(s.project_location(), window, cx);
        } else {
            self.open_project(window, cx);
        }
    }
    pub fn new_project_session(
        &mut self,
        project: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_session_at(
            omp_core::environment::ProjectLocation::local(project),
            window,
            cx,
        );
    }
    pub(in crate::desktop::workspace) fn new_session_at(
        &mut self,
        location: omp_core::environment::ProjectLocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_disabled {
            return;
        }
        if matches!(&location.path, omp_core::environment::HostPath::Local(project) if !project.is_dir())
        {
            self.runtime.error = Some("项目目录不存在".into());
            cx.notify();
            return;
        }
        if !self.guard_documents(
            documents::LeaveAction::NewSession(location.clone()),
            window,
            cx,
        ) {
            return;
        }
        let mut session = Session::new(match &location.path {
            omp_core::environment::HostPath::Local(project) => project_path(project),
            _ => PathBuf::new(),
        });
        session.location = Some(location.clone());
        if !self.demo {
            let Some(store) = self.session_sync.store.as_ref() else {
                self.session_sync.pending_create = Some(location);
                self.scan_sessions();
                self.runtime.status = "正在同步 OMP 项目会话…".into();
                cx.notify();
                return;
            };
            self.create_native_session(session, store.clone(), cx);
            return;
        }
        self.collapsed_projects.remove(&location);
        self.db.sessions.push(session);
        crate::infrastructure::filesystem::project_paths::normalize_workspace(&mut self.db);
        self.activate(self.db.sessions.len() - 1, window, cx);
    }
    pub fn manage_session(
        &mut self,
        sid: &str,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_disabled {
            return;
        }
        let Some(ix) = self.db.sessions.iter().position(|s| s.id == sid) else {
            return;
        };
        if (self.active == Some(ix) && self.runtime.starting)
            || self
                .background
                .get(sid)
                .is_some_and(|runtime| runtime.starting)
        {
            self.runtime.error = Some("会话正在连接，请在连接结束后再归档或删除。".into());
            cx.notify();
            return;
        }
        if self.active == Some(ix)
            && (delete || !self.db.sessions[ix].archived)
            && !self.guard_documents(
                documents::LeaveAction::ManageSession {
                    id: sid.to_owned(),
                    delete,
                },
                window,
                cx,
            )
        {
            return;
        }
        if self.session_is_running(sid) {
            self.runtime.error = Some("请先停止该会话，再归档或删除。".into());
            cx.notify();
            return;
        }
        if delete && !self.demo {
            let Some(store) = self.session_sync.store.clone() else {
                self.runtime.error = Some("OMP 会话尚未同步".into());
                cx.notify();
                return;
            };
            self.delete_native_session(self.db.sessions[ix].clone(), store, false);
            cx.notify();
            return;
        }
        self.background.remove(sid);
        self.manage_local_session(sid, delete, window, cx);
    }
    pub(in crate::desktop::workspace) fn remove_local_session(
        &mut self,
        sid: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session().is_some_and(|s| s.id == sid) && self.has_dirty_documents(cx) {
            self.runtime.status = "原生会话已移除，未保存的文件仍保留".into();
            return;
        }
        self.manage_local_session(sid, true, window, cx);
    }
    pub(in crate::desktop::workspace) fn manage_local_session(
        &mut self,
        sid: &str,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.db.sessions.iter().position(|s| s.id == sid) else {
            return;
        };
        let active_id = self.session().map(|s| s.id.clone());
        let clear = self.active == Some(ix) && (delete || !self.db.sessions[ix].archived);
        if delete {
            self.background.remove(sid);
            self.db.remove_session(sid);
            if !self.save_disabled
                && let Err(error) = crate::infrastructure::filesystem::attachments::remove_session(
                    &storage::data_dir().join("attachments"),
                    sid,
                )
            {
                self.runtime.error = Some(format!("会话已移除，但附件缓存暂未清理：{error:#}"));
            }
        } else {
            self.db.sessions[ix].archived = !self.db.sessions[ix].archived;
        }
        if clear {
            if !delete {
                // Preserve the release receipt under the archived session ID
                // so an immediate delete can wait for our writer to exit.
                self.switch_session_runtime("", window, cx);
            }
            self.runtime.engine = None;
            self.runtime.connected = false;
            self.runtime.starting = false;
            self.runtime.running = false;
            self.runtime.pending_send = None;
            self.runtime.requests.clear();
            self.runtime.todos.clear();
            self.runtime.agents.clear();
            self.runtime.collaboration = Default::default();
            self.active = None;
            self.db.active = None;
            self.runtime.texts.clear();
            self.search_state.hits.clear();
            self.search_state.rendered_text.clear();
            self.runtime.composer_state.commands.clear();
            self.document_state.files.clear();
            self.runtime.transcript.clear();
            self.runtime.tool_diffs.clear();
            self.runtime.list.reset(0);
            self.clear_file_preview();
            self.clear_documents();
            self.runtime
                .composer_state
                .input
                .update(cx, |i, cx| i.set_value("", window, cx));
        } else {
            self.active = active_id.and_then(|id| self.db.sessions.iter().position(|s| s.id == id));
        }
        self.persist();
        cx.notify();
    }
    pub fn request_delete_session(&mut self, sid: String, w: &mut Window, cx: &mut Context<Self>) {
        use gpui_kit::component::{
            button::{Button, ButtonVariants},
            dialog::{DialogAction, DialogClose, DialogFooter},
        };
        let weak = cx.entity().downgrade();
        w.open_dialog(cx,move|d,_,_|{
            let weak=weak.clone();let sid=sid.clone();
            d.title("删除会话？").child("将同步删除 OMP 原生会话及其附件，并移除 GUI 中的消息、评论和历史快照。项目文件不会删除；终端中也将无法再找到此会话。")
                .footer(DialogFooter::new().child(DialogClose::new().trigger(|b|b.label("取消")))
                    .child(DialogAction::new().child(Button::new("confirm-delete-session").danger().label("删除会话"))))
                .on_ok(move|_,w,cx|{let _=weak.update(cx,|this,cx|this.manage_session(&sid,true,w,cx));true})
        });
    }
}

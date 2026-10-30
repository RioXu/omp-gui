use super::*;
use crate::infrastructure::filesystem::project_paths::project_path;

impl Workspace {
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            self.error = Some("当前窗口为只读状态，无法发送或保存。请先解决数据目录问题。".into());
            cx.notify();
            return;
        }
        if self.stopping || self.pending_send.is_some() || self.steering_request.is_some() {
            return;
        }
        let text = self
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
        if text.trim().is_empty() && !s.comments.iter().any(|c| !c.sent) {
            return;
        }
        let prompt = s.prompt(&text, self.follow_up.as_deref());
        if self.demo {
            let follow = self.follow_up.take();
            if let Some(s) = self.session_mut() {
                s.submit(text, follow);
            }
            self.composer_state
                .input
                .update(cx, |i, cx| i.set_value("", window, cx));
            self.status = "演示：已保留评论记录，未调用模型".into();
            self.sync_texts(cx);
            self.persist();
            cx.notify();
            return;
        }
        if !self.connected {
            self.error = Some("OMP 尚未连接，请稍后发送或在设置中重新连接".into());
            cx.notify();
            return;
        }
        let request_id = id();
        if !self.running {
            self.in_flight_comments.clear();
            self.submitted_inputs.clear();
            self.interrupted_run = false;
            self.run_message_start = self.session().map(|s| s.messages.len()).unwrap_or(0);
        }
        if let Some(e) = &self.engine {
            let mut frame = json!({"type":"prompt","id":request_id,"message":prompt});
            if self.running {
                frame["streamingBehavior"] = json!("followUp");
            }
            match e.send(frame) {
                Ok(()) => {
                    self.pending_send = Some(PendingSubmission {
                        id: request_id,
                        text,
                        follow_up: self.follow_up.take(),
                        was_running: self.running,
                        wire_text: prompt,
                        delivered: false,
                    });
                    self.composer_state
                        .input
                        .update(cx, |i, cx| i.set_value("", window, cx));
                    self.running = true;
                    self.status = "正在提交指令…".into();
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        cx.notify();
    }
    pub fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.save_disabled {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 OMP 工作目录".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let path = project_path(path);
                let _ = this.update_in(cx, |this, window, cx| {
                    if !this.db.projects.contains(&path) {
                        this.db.projects.push(path);
                    }
                    crate::infrastructure::filesystem::project_paths::normalize_workspace(
                        &mut this.db,
                    );
                    this.persist();
                    let _ = window;
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.db.sessions.get(ix).is_none_or(|s| s.deleted) {
            return;
        }
        self.persist();
        let target = self.db.sessions[ix].id.clone();
        self.db.discard_blank_sessions(Some(&target));
        let ix = self
            .db
            .sessions
            .iter()
            .position(|s| s.id == target)
            .unwrap();
        self.active = Some(ix);
        self.db.active = Some(self.db.sessions[ix].id.clone());
        self.texts.clear();
        self.search_state.hits.clear();
        self.search_state.index = 0;
        self.composer_state.dismissed = false;
        self.transcript.clear();
        self.tool_diffs.clear();
        self.expanded_process.clear();
        self.list.reset(0);
        self.clear_file_preview();
        self.document_state.tabs.clear();
        self.document_state.active = None;
        self.settings_view = None;
        self.follow_up = None;
        self.document_state.files.clear();
        self.document_state.changes.clear();
        self.todos.clear();
        self.agents.clear();
        self.error = None;
        self.composer_state.input.update(cx, |i, cx| {
            i.set_value(self.db.sessions[ix].draft.clone(), window, cx)
        });
        self.sync_texts(cx);
        self.connect();
        self.refresh_files(cx);
        self.persist();
        cx.notify();
    }
    pub fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.save_disabled {
            return;
        }
        if let Some(s) = self.session() {
            self.new_project_session(s.project.clone(), window, cx);
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
        if self.running || self.save_disabled {
            return;
        }
        if !project.is_dir() {
            self.error = Some("项目目录不存在".into());
            cx.notify();
            return;
        }
        self.db.sessions.push(Session::new(project_path(&project)));
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
        if self.running || self.save_disabled {
            return;
        }
        let Some(ix) = self.db.sessions.iter().position(|s| s.id == sid) else {
            return;
        };
        let active_id = self.session().map(|s| s.id.clone());
        let clear = self.active == Some(ix) && (delete || !self.db.sessions[ix].archived);
        if delete {
            self.db.remove_session(sid);
        } else {
            self.db.sessions[ix].archived = !self.db.sessions[ix].archived;
        }
        if clear {
            self.engine = None;
            self.connected = false;
            self.starting = false;
            self.active = None;
            self.db.active = None;
            self.texts.clear();
            self.search_state.hits.clear();
            self.search_state.rendered_text.clear();
            self.composer_state.commands.clear();
            self.document_state.files.clear();
            self.transcript.clear();
            self.tool_diffs.clear();
            self.list.reset(0);
            self.clear_file_preview();
            self.document_state.tabs.clear();
            self.document_state.active = None;
            self.composer_state
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
            d.title("删除会话？").child("将移除 GUI 中的消息、评论和历史快照。项目文件及 OMP 独立保存的原生会话文件不会删除。")
                .footer(DialogFooter::new().child(DialogClose::new().trigger(|b|b.label("取消")))
                    .child(DialogAction::new().child(Button::new("confirm-delete-session").danger().label("删除会话"))))
                .on_ok(move|_,w,cx|{let _=weak.update(cx,|this,cx|this.manage_session(&sid,true,w,cx));true})
        });
    }
}

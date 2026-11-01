//! Environment selection owns connections; sessions retain their original host.
use super::*;
use omp_core::{
    environment::{Connection, Environment, ProjectLocation},
    host::ExecutionHost,
    remote::{RemoteHost, protocol::Operation},
};
use std::sync::Arc;

enum EnvironmentEvent {
    Progress(String, String),
    Connected(Environment, Result<(ExecutionHost, Value), String>),
    Created {
        location: ProjectLocation,
        previous: Option<String>,
        result: Result<Box<Session>, String>,
    },
    Deleted {
        id: String,
        empty_only: bool,
        result: Result<bool, String>,
    },
}

pub(super) struct EnvironmentState {
    hosts: HashMap<String, ExecutionHost>,
    connecting: std::collections::HashSet<String>,
    creating: std::collections::HashSet<ProjectLocation>,
    deleting: std::collections::HashSet<String>,
    notices: HashMap<String, String>,
    tx: mpsc::Sender<EnvironmentEvent>,
    rx: mpsc::Receiver<EnvironmentEvent>,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            hosts: HashMap::from([("local".into(), ExecutionHost::Local)]),
            connecting: Default::default(),
            creating: Default::default(),
            deleting: Default::default(),
            notices: Default::default(),
            tx,
            rx,
        }
    }
}

impl Workspace {
    pub(super) fn environment(&self) -> &Environment {
        self.db
            .environments
            .iter()
            .find(|environment| environment.id == self.db.active_environment)
            .unwrap_or(&self.db.environments[0])
    }

    pub(super) fn execution_host(&self) -> anyhow::Result<ExecutionHost> {
        let id = self
            .session()
            .map_or(self.db.active_environment.as_str(), Session::environment_id);
        self.environments
            .hosts
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("远程环境尚未连接，请从环境菜单重新连接"))
    }

    pub(super) fn environment_projects(&self) -> Vec<ProjectLocation> {
        if self.db.active_environment == "local" {
            self.db
                .projects
                .iter()
                .cloned()
                .map(ProjectLocation::local)
                .collect()
        } else {
            self.db
                .remote_projects
                .iter()
                .filter(|project| {
                    project.environment == self.db.active_environment
                        && project.profile == self.environment().profile
                })
                .cloned()
                .collect()
        }
    }

    pub(super) fn environment_omp(&self) -> String {
        if self.db.active_environment == "local" {
            self.db.omp_path.clone()
        } else {
            self.environment()
                .omp_override
                .clone()
                .unwrap_or_else(|| "omp".into())
        }
    }

    pub(super) fn switch_environment(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_disabled
            || !self
                .db
                .environments
                .iter()
                .any(|environment| environment.id == id)
        {
            return;
        }
        if id == self.db.active_environment {
            self.connect_environment(cx);
            return;
        }
        if !self.guard_documents(
            documents::LeaveAction::SwitchEnvironment(id.into()),
            window,
            cx,
        ) {
            return;
        }
        self.persist();
        // Park running state before clearing the visible session; no task is stopped.
        self.switch_session_runtime("", window, cx);
        self.active = None;
        self.db.active = None;
        self.db.active_environment = id.into();
        self.clear_documents();
        self.clear_file_preview();
        self.document_state.files.clear();
        self.document_state.changes.clear();
        self.settings_view = None;
        self.search_state.hits.clear();
        self.session_sync = Default::default();
        self.connect_environment(cx);
        self.runtime.error = self.environments.notices.remove(id);
        self.scan_sessions();
        self.persist();
        cx.notify();
    }

    pub(super) fn connect_environment(&mut self, cx: &mut Context<Self>) {
        let environment = self.environment().clone();
        let Connection::Ssh { host } = &environment.connection else {
            return;
        };
        if self.environments.connecting.contains(&environment.id) {
            return;
        }
        self.environments.connecting.insert(environment.id.clone());
        self.runtime.status = format!("正在连接 SSH：{}…", environment.name);
        let host = host.clone();
        let helper_override = environment
            .helper
            .clone()
            .or_else(|| std::env::var("OMP_GUI_REMOTE_HELPER").ok());
        let tx = self.environments.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let helper = match helper_override {
                    Some(helper) => helper,
                    None => crate::desktop::remote_install::ensure(&host, |stage| {
                        let _ = tx.send(EnvironmentEvent::Progress(
                            environment.id.clone(),
                            stage.into(),
                        ));
                    })?,
                };
                let remote =
                    RemoteHost::connect_profile(&host, &helper, environment.profile.as_deref())?;
                let hello: Value = remote.call(Operation::Hello {
                    omp: environment
                        .omp_override
                        .clone()
                        .unwrap_or_else(|| "omp".into()),
                })?;
                anyhow::ensure!(hello["os"] == "linux", "远端服务必须运行在 Linux");
                anyhow::ensure!(
                    hello["version"] == env!("CARGO_PKG_VERSION"),
                    "远端服务版本与 GUI 不一致，请更新服务或清空服务路径覆盖后重连"
                );
                Ok((ExecutionHost::Remote(Arc::new(remote)), hello))
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(EnvironmentEvent::Connected(environment, result));
        });
        cx.notify();
    }

    pub(super) fn poll_environments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for event in self.environments.rx.try_iter().collect::<Vec<_>>() {
            match event {
                EnvironmentEvent::Progress(environment, stage) => {
                    if self.db.active_environment == environment {
                        self.runtime.status = stage;
                        cx.notify();
                    }
                }
                EnvironmentEvent::Created {
                    location,
                    previous,
                    result,
                } => {
                    self.environments.creating.remove(&location);
                    match result {
                        Ok(session) => {
                            let existing = self.db.sessions.iter().position(|saved| {
                                saved.environment_id() == session.environment_id()
                                    && saved.native_id == session.native_id
                                    && saved.native_root == session.native_root
                            });
                            let ix = existing.unwrap_or_else(|| {
                                self.db.sessions.push(*session);
                                self.db.sessions.len() - 1
                            });
                            self.collapsed_projects.remove(&location);
                            if self.db.active_environment == location.environment
                                && self.db.active == previous
                            {
                                self.activate(ix, window, cx);
                            }
                            self.persist();
                        }
                        Err(error) if location.environment == self.db.active_environment => {
                            self.runtime.error = Some(error)
                        }
                        Err(error) => {
                            self.environments
                                .notices
                                .insert(location.environment, error);
                        }
                    }
                    cx.notify();
                }
                EnvironmentEvent::Deleted {
                    id,
                    empty_only,
                    result,
                } => {
                    self.environments.deleting.remove(&id);
                    if self.session().is_some_and(|session| session.id == id) {
                        self.runtime.status = if result.is_err() {
                            "删除失败，会话已保留"
                        } else {
                            "删除检查已结束"
                        }
                        .into();
                    }
                    match result {
                        Ok(true) => self.remove_local_session(&id, window, cx),
                        Ok(false) => {}
                        Err(error) => {
                            let error = format!(
                                "{}，已保留会话：{error}",
                                if empty_only {
                                    "空白会话暂未清理"
                                } else {
                                    "删除未确认"
                                }
                            );
                            let owner = self
                                .db
                                .sessions
                                .iter()
                                .find(|session| session.id == id)
                                .map(|session| session.environment_id().to_owned());
                            if let Some(owner) =
                                owner.filter(|owner| owner != &self.db.active_environment)
                            {
                                self.environments.notices.insert(owner, error);
                            } else {
                                self.runtime.error = Some(error);
                            }
                        }
                    }
                    cx.notify();
                }
                EnvironmentEvent::Connected(environment, result) => {
                    if !self
                        .db
                        .environments
                        .iter()
                        .any(|saved| saved == &environment)
                    {
                        continue;
                    }
                    self.environments.connecting.remove(&environment.id);
                    match result {
                        Ok((host, hello)) => {
                            self.environments.hosts.insert(environment.id.clone(), host);
                            if self.db.active_environment == environment.id {
                                self.runtime.status = format!(
                                    "SSH：{} · {}",
                                    environment.name,
                                    hello["omp_version"].as_str().unwrap_or("OMP")
                                );
                                self.runtime.error = None;
                                let pending = self.session_sync.pending_create.take();
                                self.session_sync = Default::default();
                                self.session_sync.pending_create = pending;
                                self.scan_sessions();
                            }
                        }
                        Err(error) if self.db.active_environment == environment.id => {
                            self.runtime.error = Some(error);
                            self.runtime.status = "远程连接失败".into();
                        }
                        Err(error) => {
                            self.environments.notices.insert(environment.id, error);
                        }
                    }
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn creating_session(&self) -> bool {
        self.environments
            .creating
            .iter()
            .any(|project| project.environment == self.db.active_environment)
    }
    pub(super) fn deleting_active_session(&self) -> bool {
        self.session()
            .is_some_and(|session| self.environments.deleting.contains(&session.id))
    }
    pub(super) fn connecting_environment(&self) -> bool {
        self.environments
            .connecting
            .contains(&self.db.active_environment)
    }

    pub(super) fn create_native_session(
        &mut self,
        mut session: Session,
        store: omp_core::host::NativeStore,
        cx: &mut Context<Self>,
    ) {
        let location = session.project_location();
        if !self.environments.creating.insert(location.clone()) {
            return;
        }
        let previous = self.db.active.clone();
        let tx = self.environments.tx.clone();
        self.runtime.status = "正在新建会话…".into();
        std::thread::spawn(move || {
            let result = store
                .create(&session.host_cwd(), &session.title)
                .map(|native| {
                    session.native_root = Some(store.root.text());
                    session.native_id = Some(native.id);
                    session.native_revision = Some(native.revision);
                    session.omp_session = Some(native.path.to_string_lossy().into_owned());
                    Box::new(session)
                })
                .map_err(|error| format!("创建原生会话失败：{error:#}"));
            let _ = tx.send(EnvironmentEvent::Created {
                location,
                previous,
                result,
            });
        });
        cx.notify();
    }

    pub(super) fn delete_native_session(
        &mut self,
        session: Session,
        store: omp_core::host::NativeStore,
        empty_only: bool,
    ) {
        if !self.environments.deleting.insert(session.id.clone()) {
            return;
        }
        if self.session().is_some_and(|owner| owner.id == session.id) {
            self.runtime.status = "正在删除会话…".into();
        }
        let tx = self.environments.tx.clone();
        let legacy = storage::data_dir().join("omp-sessions");
        let connecting = if self.session().is_some_and(|owner| owner.id == session.id) {
            self.runtime.connection_result.take()
        } else {
            self.background
                .get_mut(&session.id)
                .and_then(|runtime| runtime.connection_result.take())
        };
        std::thread::spawn(move || {
            if let Some(connecting) = connecting {
                match connecting.recv_timeout(Duration::from_secs(65)) {
                    Ok(connection) => drop(connection),
                    Err(error) => {
                        let _ = tx.send(EnvironmentEvent::Deleted {
                            id: session.id,
                            empty_only,
                            result: Err(format!("连接初始化未确认结束：{error}")),
                        });
                        return;
                    }
                }
            }
            let deadline = Instant::now()
                + Duration::from_secs(if session.environment_id() == "local" {
                    5
                } else {
                    20
                });
            while store
                .available(
                    &session.host_cwd(),
                    session.native_id.as_deref().unwrap_or_default(),
                )
                .is_err()
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            let result = if empty_only {
                store.discard_if_untouched(&session, &legacy)
            } else {
                store.delete(&session, &legacy).map(|()| true)
            }
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(EnvironmentEvent::Deleted {
                id: session.id,
                empty_only,
                result,
            });
        });
    }

    pub(super) fn manage_connection(
        &mut self,
        existing: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_kit::component::{
            button::{Button, ButtonVariants},
            dialog::{DialogAction, DialogClose, DialogFooter},
            input::Input,
        };
        let previous = existing.and_then(|id| {
            self.db
                .environments
                .iter()
                .find(|environment| environment.id == id && id != "local")
                .cloned()
        });
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("显示名称（可选）")
                .default_value(
                    previous
                        .as_ref()
                        .map(|e| e.name.clone())
                        .unwrap_or_default(),
                )
        });
        let host = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("SSH Host 别名或 user@host")
                .default_value(
                    previous
                        .as_ref()
                        .and_then(|e| match &e.connection {
                            Connection::Ssh { host } => Some(host.clone()),
                            _ => None,
                        })
                        .unwrap_or_default(),
                )
        });
        let omp = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("自动从远端环境查找；可填写绝对路径")
                .default_value(
                    previous
                        .as_ref()
                        .and_then(|e| e.omp_override.clone())
                        .unwrap_or_default(),
                )
        });
        let helper = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("自动安装路径；可覆盖远端服务路径")
                .default_value(
                    previous
                        .as_ref()
                        .and_then(|e| e.helper.clone())
                        .unwrap_or_default(),
                )
        });
        let profile = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("使用远端默认 profile；可填写绝对路径")
                .default_value(
                    previous
                        .as_ref()
                        .and_then(|e| e.profile.clone())
                        .unwrap_or_default(),
                )
        });
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let name = name.clone();
            let host = host.clone();
            let omp = omp.clone();
            let helper = helper.clone();
            let profile = profile.clone();
            let weak = weak.clone();
            let previous = previous.clone();
            dialog
                .title(if previous.is_some() {
                    "编辑 SSH 连接"
                } else {
                    "新增 SSH 连接"
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(Input::new(&name))
                        .child(Input::new(&host))
                        .child(div().text_sm().child("远端 OMP 路径覆盖"))
                        .child(Input::new(&omp))
                        .child(
                            div()
                                .text_sm()
                                .child("远端 OMP profile（PI_CODING_AGENT_DIR）"),
                        )
                        .child(Input::new(&profile))
                        .child(div().text_sm().child("远端服务路径覆盖"))
                        .child(Input::new(&helper)),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("取消")))
                        .child(
                            DialogAction::new().child(
                                Button::new("save-ssh-connection")
                                    .primary()
                                    .label("保存并连接"),
                            ),
                        ),
                )
                .on_ok(move |_, w, cx| {
                    let host = host.read(cx).value().trim().to_owned();
                    if host.is_empty() {
                        return false;
                    }
                    let name = name.read(cx).value().trim().to_owned();
                    let override_path = omp.read(cx).value().trim().to_owned();
                    let helper_path = helper.read(cx).value().trim().to_owned();
                    let profile = profile.read(cx).value().trim().to_owned();
                    if !profile.is_empty()
                        && omp_core::environment::HostPath::posix(&profile).is_err()
                    {
                        return false;
                    }
                    let mut environment = Environment {
                        id: previous.as_ref().map(|e| e.id.clone()).unwrap_or_else(id),
                        name: if name.is_empty() { host.clone() } else { name },
                        connection: Connection::Ssh { host },
                        omp_override: (!override_path.is_empty()).then_some(override_path),
                        profile: (!profile.is_empty()).then_some(profile),
                        helper: (!helper_path.is_empty()).then_some(helper_path),
                    };
                    if previous
                        .as_ref()
                        .is_some_and(|saved| !saved.same_target(&environment))
                    {
                        environment.id = id();
                    }
                    let _ = weak.update(cx, |this, cx| {
                        let environment_id = environment.id.clone();
                        if let Some(saved) = this
                            .db
                            .environments
                            .iter_mut()
                            .find(|saved| saved.id == environment_id)
                        {
                            *saved = environment;
                        } else {
                            this.db.environments.push(environment);
                        }
                        this.environments.hosts.remove(&environment_id);
                        this.persist();
                        this.switch_environment(&environment_id, w, cx);
                    });
                    true
                })
        });
    }
}

#[cfg(test)]
mod tests;

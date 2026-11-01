use super::*;
use crate::infrastructure::{
    filesystem::project_paths::project_key,
    omp::sessions::{NativeSession, Scan},
};
use omp_core::{
    environment::{HostPath, ProjectLocation},
    host::NativeStore as SessionStore,
};
use std::collections::HashSet;

type ScanResult = Result<(SessionStore, Scan, Instant), String>;
pub(in crate::desktop::workspace) const SYNC_STATUS: &str = "正在同步 OMP 项目会话…";
pub(in crate::desktop::workspace) struct SessionSync {
    pub store: Option<SessionStore>,
    tx: mpsc::Sender<ScanResult>,
    rx: mpsc::Receiver<ScanResult>,
    scanning: bool,
    last_scan: Instant,
    missing: HashSet<String>,
    pub pending_create: Option<ProjectLocation>,
    executable: String,
    last_error: Option<String>,
}
impl Default for SessionSync {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            store: None,
            tx,
            rx,
            scanning: false,
            last_scan: Instant::now() - Duration::from_secs(5),
            missing: HashSet::new(),
            pending_create: None,
            executable: String::new(),
            last_error: None,
        }
    }
}
impl Workspace {
    pub(in crate::desktop::workspace) fn syncing_sessions(&self) -> bool {
        self.session_sync.scanning && self.runtime.status == SYNC_STATUS
    }
    pub(in crate::desktop::workspace) fn scan_sessions(&mut self) {
        if self.demo || self.save_disabled || self.session_sync.scanning {
            return;
        }
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(_) => return,
        };
        let environment = self.db.active_environment.clone();
        let omp = self.environment_omp();
        self.session_sync.scanning = true;
        self.session_sync.last_scan = Instant::now();
        if self.session_sync.store.is_none() && !self.runtime.running {
            self.runtime.status = SYNC_STATUS.into();
        }
        if self.session_sync.executable != omp {
            self.session_sync.store = None;
            self.session_sync.executable = omp.clone();
        }
        let store = self.session_sync.store.clone();
        let mut projects = self
            .environment_projects()
            .into_iter()
            .map(|project| project.path)
            .collect::<Vec<_>>();
        projects.extend(
            self.db
                .sessions
                .iter()
                .filter(|s| s.environment_id() == environment)
                .map(Session::host_cwd),
        );
        let cwd = projects.first().cloned().unwrap_or_else(|| {
            if environment == "local" {
                HostPath::Local(std::env::current_dir().unwrap_or_default())
            } else {
                HostPath::Posix("/".into())
            }
        });
        let sessions: Vec<_> = self
            .db
            .sessions
            .iter()
            .filter(|s| s.environment_id() == environment)
            .map(crate::infrastructure::omp::sessions::Registration::from)
            .collect();
        let active = self
            .session()
            .filter(|_| !self.runtime.running && self.runtime.pending_send.is_none())
            .map(|session| session.id.clone());
        let began = Instant::now();
        let tx = self.session_sync.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let store = match store {
                    Some(s) => s,
                    None => SessionStore::discover(host, &omp, &cwd)?,
                };
                let scan = store.sync(
                    &projects,
                    &sessions,
                    active.as_deref(),
                    &storage::data_dir().join("omp-sessions"),
                )?;
                Ok((store, scan, began))
            })();
            let _ = tx.send(result.map_err(|e| format!("会话同步失败：{e:#}")));
        });
    }
    pub(in crate::desktop::workspace) fn poll_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let results: Vec<_> = self.session_sync.rx.try_iter().collect();
        for result in results {
            self.session_sync.scanning = false;
            cx.notify();
            if self.session_sync.executable != self.environment_omp() {
                self.session_sync.store = None;
                self.session_sync.last_scan = Instant::now() - Duration::from_secs(5);
                continue;
            }
            let (store, scan, began) = match result {
                Ok(r) => r,
                Err(e) => {
                    self.session_sync.last_error = Some(e.clone());
                    if self.runtime.status == SYNC_STATUS {
                        self.runtime.status = "会话同步失败，正在等待重试".into();
                    }
                    if self.runtime.error.as_ref() != Some(&e) {
                        self.runtime.error = Some(e);
                        cx.notify();
                    }
                    continue;
                }
            };
            if self.runtime.error.as_ref() == self.session_sync.last_error.as_ref() {
                self.runtime.error = None;
            }
            self.session_sync.last_error = None;
            let first = self.session_sync.store.is_none();
            let remote = matches!(store.root, HostPath::Posix(_));
            let environment = self.db.active_environment.clone();
            self.session_sync.store = Some(store.clone());
            let active_id = self.session().map(|s| s.id.clone());
            let mut changed = false;
            for (id, native) in scan.bindings {
                if let Some(s) = self
                    .db
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == id && s.native_root.is_none())
                {
                    bind(s, &native, &store.root);
                    changed = true;
                }
            }
            for native in scan.sessions {
                if !remote && !native.path.is_file() {
                    continue;
                }
                let existing = self.db.sessions.iter().position(|s| {
                    s.environment_id() == environment
                        && s.native_root.as_deref() == Some(store.root.text().as_str())
                        && (s.native_id.as_ref() == Some(&native.id)
                            || s.omp_session.as_deref().is_some_and(|p| {
                                native_key(PathBuf::from(p).as_path(), remote)
                                    == native_key(&native.path, remote)
                            }))
                });
                if let Some(ix) = existing {
                    let active = self.active == Some(ix);
                    let runtime = if active {
                        Some(&self.runtime)
                    } else {
                        self.background.get(&self.db.sessions[ix].id)
                    };
                    if runtime.is_some_and(|r| r.running || r.pending_send.is_some() || r.starting)
                    {
                        continue;
                    }
                    if (if native.metadata_only {
                        self.db.sessions[ix].native_index_revision.as_ref()
                    } else {
                        self.db.sessions[ix].native_revision.as_ref()
                    }) == Some(&native.revision)
                        && self.db.sessions[ix]
                            .omp_session
                            .as_deref()
                            .is_some_and(|p| {
                                native_key(std::path::Path::new(p), remote)
                                    == native_key(&native.path, remote)
                            })
                    {
                        continue;
                    }
                    let s = &mut self.db.sessions[ix];
                    // The stream may have just settled after this worker took
                    // its snapshot. Only merge a snapshot that is still current.
                    if runtime.is_some_and(|r| began < r.last_activity) {
                        continue;
                    }
                    let before: Vec<_> = s
                        .messages
                        .iter()
                        .filter(|_| !native.metadata_only)
                        .map(|m| (m.role.clone(), m.text.clone()))
                        .collect();
                    let title = s.title.clone();
                    let path_changed = s.omp_session.as_deref().is_none_or(|p| {
                        native_key(std::path::Path::new(p), remote)
                            != native_key(&native.path, remote)
                    });
                    bind(s, &native, &store.root);
                    let external_change = path_changed
                        || title != s.title
                        || (!native.metadata_only
                            && before
                                != s.messages
                                    .iter()
                                    .map(|m| (m.role.clone(), m.text.clone()))
                                    .collect::<Vec<_>>());
                    if active && external_change && self.runtime.engine.is_some() {
                        self.runtime.engine = None;
                        self.runtime.connected = false;
                    } else if !active && external_change {
                        self.background.remove(&self.db.sessions[ix].id);
                    }
                    changed = true;
                } else {
                    // A published migration may still be waiting for the
                    // terminal owner to release its lease. Preserve that GUI
                    // row and its comments rather than importing a duplicate.
                    if self.db.sessions.iter().any(|s| {
                        s.environment_id() == environment
                            && s.native_root.is_none()
                            && s.omp_session.as_deref().is_some_and(|p| {
                                std::path::Path::new(p).file_stem().is_some_and(|name| {
                                    name.to_string_lossy().ends_with(&native.id)
                                })
                            })
                    }) {
                        continue;
                    }
                    let project = self
                        .db
                        .sessions
                        .iter()
                        .find(|s| {
                            s.environment_id() == environment
                                && native_key(s.cwd(), remote) == native_key(&native.cwd, remote)
                        })
                        .map(Session::project_location)
                        .or_else(|| {
                            self.environment_projects()
                                .into_iter()
                                .find(|p| match &p.path {
                                    HostPath::Local(path) => {
                                        project_key(path) == project_key(&native.cwd)
                                    }
                                    HostPath::Posix(path) => {
                                        path == native.cwd.to_string_lossy().as_ref()
                                    }
                                })
                        });
                    if let Some(project) = project {
                        let mut s = Session::new(match &project.path {
                            HostPath::Local(path) => path.clone(),
                            _ => PathBuf::new(),
                        });
                        s.location = Some(project.clone());
                        s.updated = native.updated;
                        if matches!(&project.path, HostPath::Local(path) if project_key(path) != project_key(&native.cwd))
                        {
                            s.worktree = Some(native.cwd.clone());
                        }
                        bind(&mut s, &native, &store.root);
                        self.db.sessions.push(s);
                        changed = true;
                    }
                }
            }
            let missing: HashSet<_> = scan.missing.into_iter().collect();
            for id in &missing {
                self.background.remove(id);
            }
            if active_id.as_ref().is_some_and(|id| missing.contains(id)) {
                self.runtime.engine = None;
                self.runtime.connected = false;
                self.runtime.starting = false;
                self.runtime.running = false;
            }
            let confirmed: Vec<_> = missing
                .intersection(&self.session_sync.missing)
                .cloned()
                .collect();
            for id in confirmed {
                if self.db.sessions.iter().any(|s| s.id == id) {
                    // Stop our writer before removing the record: native OMP
                    // recreates an externally deleted journal on its next write.
                    self.remove_local_session(&id, window, cx);
                    changed = true;
                }
            }
            self.session_sync.missing = missing;
            if let Some(e) = scan.errors.first() {
                self.session_sync.last_error = Some(e.clone());
                if self.runtime.error.as_ref() != Some(e) {
                    self.runtime.error = Some(e.clone());
                    cx.notify();
                }
            }
            self.active = active_id.and_then(|id| self.db.sessions.iter().position(|s| s.id == id));
            if changed {
                if let Some(ix) = self.active {
                    self.runtime.todos = self.db.sessions[ix].todos.clone();
                    self.runtime.agents = self.db.sessions[ix].agents.clone();
                }
                self.sync_texts(cx);
                self.persist();
                cx.notify();
            }
            if first
                && self.active.is_some()
                && self
                    .session()
                    .is_some_and(|session| session.native_revision.is_some())
                && self.runtime.engine.is_none()
                && !self.runtime.running
                && self.session_sync.pending_create.is_none()
            {
                self.connect();
            }
            if let Some(project) = self.session_sync.pending_create.take() {
                self.new_session_at(project, window, cx);
            }
            // A terminal-owned session is readable. Connect after it is released.
            if !self.runtime.running
                && !self.runtime.starting
                && self.runtime.engine.is_none()
                && self.session().is_some_and(|s| {
                    s.native_root.is_some()
                        && s.native_revision.is_some()
                        && !self.session_sync.missing.contains(&s.id)
                })
            {
                self.connect();
                cx.notify();
            }
            if self.runtime.status == SYNC_STATUS && !self.runtime.starting {
                self.runtime.status = "OMP 项目会话已同步".into();
            }
        }
        if self.session_sync.last_scan.elapsed() >= Duration::from_secs(2) {
            self.scan_sessions();
        }
    }
}
fn bind(session: &mut Session, native: &NativeSession, root: &HostPath) {
    session.native_root = Some(root.text());
    session.native_id = Some(native.id.clone());
    session.native_index_revision = Some(native.revision.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    if !native.title.trim().is_empty() {
        session.title = native.title.clone();
    }
    session.updated = session.updated.max(native.updated);
    if native.metadata_only {
        return;
    }
    session.native_revision = Some(native.revision.clone());
    session.merge_native_history(native.messages.as_ref().clone());
    session.todos = native.todos.clone();
    for agent in &native.agents {
        crate::domain::activity::merge_agent(&mut session.agents, agent);
    }
}

fn native_key(path: &std::path::Path, remote: bool) -> String {
    if remote {
        path.to_string_lossy().into_owned()
    } else {
        project_key(path)
    }
}

#[cfg(test)]
mod tests {
    use super::bind;
    use crate::domain::{Message, Session};
    use crate::infrastructure::omp::sessions::NativeSession;
    use omp_core::environment::HostPath;
    #[test]
    fn metadata_refresh_does_not_erase_cached_history_or_mark_it_loaded() {
        let mut session = Session::new(Default::default());
        session
            .messages
            .push(Message::new("assistant", "cached reply".into()));
        session.native_revision = Some("loaded-old".into());
        let mut native: NativeSession = serde_json::from_value(serde_json::json!({
            "metadata_only": true,
            "id": "native",
            "path": "/profile/sessions/a.jsonl",
            "cwd": "/project",
            "title": "updated title",
            "updated": 123,
            "revision": "index-new",
            "messages": [],
            "todos": [],
            "agents": []
        }))
        .unwrap();
        bind(&mut session, &native, &HostPath::Posix("/profile".into()));
        assert_eq!(session.messages[0].text, "cached reply");
        assert_eq!(session.native_revision.as_deref(), Some("loaded-old"));
        assert_eq!(session.native_index_revision.as_deref(), Some("index-new"));
        assert_eq!(session.title, "updated title");
        native.metadata_only = false;
        bind(&mut session, &native, &HostPath::Posix("/profile".into()));
        assert!(session.messages.is_empty());
        assert_eq!(session.native_revision.as_deref(), Some("index-new"));
    }
}

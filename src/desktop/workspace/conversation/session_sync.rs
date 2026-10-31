use super::*;
use crate::infrastructure::{
    filesystem::project_paths::project_key,
    omp::sessions::{NativeSession, Registration, Scan, SessionStore},
};
use std::collections::HashSet;

type ScanResult = Result<(SessionStore, Scan, Instant), String>;
pub(in crate::desktop::workspace) struct SessionSync {
    pub store: Option<SessionStore>,
    tx: mpsc::Sender<ScanResult>,
    rx: mpsc::Receiver<ScanResult>,
    scanning: bool,
    last_scan: Instant,
    missing: HashSet<String>,
    pub pending_create: Option<PathBuf>,
    executable: String,
    pub last_activity: Instant,
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
            last_activity: Instant::now(),
        }
    }
}
impl Workspace {
    pub(in crate::desktop::workspace) fn scan_sessions(&mut self) {
        if self.demo || self.save_disabled || self.session_sync.scanning {
            return;
        }
        self.session_sync.scanning = true;
        self.session_sync.last_scan = Instant::now();
        if self.session_sync.executable != self.db.omp_path {
            self.session_sync.store = None;
            self.session_sync.executable = self.db.omp_path.clone();
        }
        let store = self.session_sync.store.clone();
        let omp = self.db.omp_path.clone();
        let mut projects = self.db.projects.clone();
        projects.extend(self.db.sessions.iter().map(|s| s.cwd().to_owned()));
        let cwd = projects
            .iter()
            .find(|p| p.is_dir())
            .cloned()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let sessions: Vec<_> = self.db.sessions.iter().map(Registration::from).collect();
        let began = Instant::now();
        let tx = self.session_sync.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let store = match store {
                    Some(s) => s,
                    None => SessionStore::discover(&omp, &cwd)?,
                };
                let scan = store.scan(
                    &projects,
                    &sessions,
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
            if self.session_sync.executable != self.db.omp_path {
                self.session_sync.store = None;
                self.session_sync.last_scan = Instant::now() - Duration::from_secs(5);
                continue;
            }
            let (store, scan, began) = match result {
                Ok(r) => r,
                Err(e) => {
                    if self.error.as_ref() != Some(&e) {
                        self.error = Some(e);
                        cx.notify();
                    }
                    continue;
                }
            };
            let first = self.session_sync.store.is_none();
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
                    bind(s, &native, &store);
                    changed = true;
                }
            }
            for native in scan.sessions {
                if !native.path.is_file() {
                    continue;
                }
                let existing = self.db.sessions.iter().position(|s| {
                    s.native_root.as_deref() == Some(store.root.to_string_lossy().as_ref())
                        && (s.native_id.as_ref() == Some(&native.id)
                            || s.omp_session.as_deref().is_some_and(|p| {
                                project_key(PathBuf::from(p).as_path()) == project_key(&native.path)
                            }))
                });
                if let Some(ix) = existing {
                    let active = self.active == Some(ix);
                    if active && (self.running || self.pending_send.is_some() || self.starting) {
                        continue;
                    }
                    if self.db.sessions[ix].native_revision.as_ref() == Some(&native.revision)
                        && self.db.sessions[ix]
                            .omp_session
                            .as_deref()
                            .is_some_and(|p| {
                                project_key(std::path::Path::new(p)) == project_key(&native.path)
                            })
                    {
                        continue;
                    }
                    let s = &mut self.db.sessions[ix];
                    // The stream may have just settled after this worker took
                    // its snapshot. Only merge a snapshot that is still current.
                    if active && began < self.session_sync.last_activity {
                        continue;
                    }
                    let before: Vec<_> = s
                        .messages
                        .iter()
                        .map(|m| (m.role.clone(), m.text.clone()))
                        .collect();
                    let title = s.title.clone();
                    let path_changed = s.omp_session.as_deref().is_none_or(|p| {
                        project_key(std::path::Path::new(p)) != project_key(&native.path)
                    });
                    bind(s, &native, &store);
                    let external_change = path_changed
                        || title != s.title
                        || before
                            != s.messages
                                .iter()
                                .map(|m| (m.role.clone(), m.text.clone()))
                                .collect::<Vec<_>>();
                    if active && external_change && self.engine.is_some() {
                        self.engine = None;
                        self.connected = false;
                    }
                    changed = true;
                } else {
                    // A published migration may still be waiting for the
                    // terminal owner to release its lease. Preserve that GUI
                    // row and its comments rather than importing a duplicate.
                    if self.db.sessions.iter().any(|s| {
                        s.native_root.is_none()
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
                        .find(|s| project_key(s.cwd()) == project_key(&native.cwd))
                        .map(|s| s.project.clone())
                        .or_else(|| {
                            self.db
                                .projects
                                .iter()
                                .find(|p| project_key(p) == project_key(&native.cwd))
                                .cloned()
                        });
                    if let Some(project) = project {
                        let mut s = Session::new(project.clone());
                        s.updated = native.updated;
                        if project_key(&project) != project_key(&native.cwd) {
                            s.worktree = Some(native.cwd.clone());
                        }
                        bind(&mut s, &native, &store);
                        self.db.sessions.push(s);
                        changed = true;
                    }
                }
            }
            let missing: HashSet<_> = scan.missing.into_iter().collect();
            if active_id.as_ref().is_some_and(|id| missing.contains(id)) {
                self.engine = None;
                self.connected = false;
                self.starting = false;
                self.running = false;
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
            if let Some(e) = scan.errors.first()
                && self.error.as_ref() != Some(e)
            {
                self.error = Some(e.clone());
                cx.notify();
            }
            self.active = active_id.and_then(|id| self.db.sessions.iter().position(|s| s.id == id));
            if changed {
                if let Some(ix) = self.active {
                    self.todos = self.db.sessions[ix].todos.clone();
                    self.agents = self.db.sessions[ix].agents.clone();
                }
                self.sync_texts(cx);
                self.persist();
                cx.notify();
            }
            if first
                && self.active.is_some()
                && self.engine.is_none()
                && !self.running
                && self.session_sync.pending_create.is_none()
            {
                self.connect();
            }
            if let Some(project) = self.session_sync.pending_create.take() {
                self.new_project_session(project, window, cx);
            }
            // A terminal-owned session is readable. Connect after it is released.
            if !self.running
                && !self.starting
                && self.engine.is_none()
                && self.session().is_some_and(|s| {
                    s.native_root.is_some() && !self.session_sync.missing.contains(&s.id)
                })
            {
                self.connect();
                cx.notify();
            }
        }
        if self.session_sync.last_scan.elapsed() >= Duration::from_secs(2) {
            self.scan_sessions();
        }
    }
}
fn bind(session: &mut Session, native: &NativeSession, store: &SessionStore) {
    session.native_root = Some(store.root.to_string_lossy().into_owned());
    session.native_id = Some(native.id.clone());
    session.native_revision = Some(native.revision.clone());
    session.omp_session = Some(native.path.to_string_lossy().into_owned());
    if !native.title.trim().is_empty() {
        session.title = native.title.clone();
    }
    session.updated = session.updated.max(native.updated);
    session.merge_native_history(native.messages.as_ref().clone());
    session.todos = native.todos.clone();
    for agent in &native.agents {
        crate::domain::activity::merge_agent(&mut session.agents, agent);
    }
}

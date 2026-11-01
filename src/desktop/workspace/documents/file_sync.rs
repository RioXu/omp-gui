use super::*;
use crate::infrastructure::filesystem::changes::{ChangeKind, ChangedFile};

use omp_core::host::ProjectFiles;

impl Workspace {
    pub fn clear_file_preview(&mut self) {
        self.document_state.text = None;
        self.document_state.diff = None;
        self.document_state.quote = None;
        self.document_state.path = None;
        self.document_state.snapshot = None;
        self.document_state.historical = false;
        self.selected = None;
    }

    pub fn select_panel(&mut self, tab: &'static str) {
        self.panel_tab = tab;
        self.panel_open = true;
    }

    pub fn close_panel(&mut self) {
        self.panel_open = false;
    }

    pub(in crate::desktop::workspace) fn watch_files(&mut self) {
        let Some(session) = self.session() else {
            self.document_state.watcher = None;
            self.document_state.watched_session.clear();
            return;
        };
        let sid = session.id.clone();
        let root = session.host_cwd();
        if self
            .document_state
            .watch_retry
            .is_some_and(|retry| Instant::now() < retry)
        {
            return;
        }
        self.document_state.watch_retry = None;
        let mut external = self
            .document_state
            .tabs
            .iter()
            .filter(|tab| !tab.historical)
            .filter_map(|tab| tab.save_path.as_ref())
            .filter(|path| !path.is_within(&root))
            .filter_map(omp_core::environment::HostPath::parent)
            .collect::<Vec<_>>();
        external.sort_by_key(omp_core::environment::HostPath::text);
        external.dedup();
        let signature = format!(
            "{sid}|{}",
            external
                .iter()
                .map(omp_core::environment::HostPath::text)
                .collect::<Vec<_>>()
                .join("|")
        );
        if signature == self.document_state.watched_session {
            let events = self
                .document_state
                .watcher
                .as_ref()
                .map(|watcher| watcher.poll())
                .unwrap_or_default();
            for event in events {
                match event {
                    Ok(change) => {
                        let _ = self.document_state.tx.send(FileEvent::Changed {
                            session: sid.clone(),
                            links_changed: change.links_changed,
                        });
                    }
                    Err(_) => {
                        self.document_state.watcher = None;
                        self.document_state.watched_session.clear();
                        self.document_state.watch_retry =
                            Some(Instant::now() + Duration::from_secs(5));
                    }
                }
            }
            return;
        }
        let host = if self.demo {
            None
        } else {
            match self.execution_host() {
                Ok(host) => Some(host),
                Err(_) => return,
            }
        };
        self.document_state.watcher = None;
        self.document_state.watched_session = signature.clone();
        self.document_state.loading = false;
        self.document_state.refresh_pending = Some(Instant::now());
        let Some(host) = host else {
            return;
        };
        let tx = self.document_state.tx.clone();
        std::thread::spawn(move || {
            let result = host
                .watch(&root, &external)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(FileEvent::WatchReady {
                session: sid,
                signature,
                result,
            });
        });
    }
    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.polling_background || self.connecting_environment() {
            return;
        }
        if self.document_state.loading {
            return;
        }
        self.document_state.last_scan = Instant::now();
        self.document_state.refresh_pending = None;
        if self.demo {
            if let Some(s) = self.session() {
                self.document_state.changes = s
                    .snapshots
                    .values()
                    .map(|s| ChangedFile {
                        path: s.path.clone(),
                        kind: ChangeKind::Modified,
                    })
                    .collect();
                self.document_state
                    .changes
                    .sort_by(|a, b| a.path.cmp(&b.path));
                self.document_state.changes.dedup();
                self.document_state.files = self
                    .document_state
                    .changes
                    .iter()
                    .map(|c| c.path.clone())
                    .collect();
                self.document_state.tree.update(cx, |t, cx| {
                    crate::desktop::workspace::documents::file_tree::update(
                        t,
                        &self.document_state.files,
                        cx,
                    )
                });
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let project = s.host_cwd();
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(error) => {
                self.runtime.error = Some(format!("{error:#}"));
                return;
            }
        };
        let sid = s.id.clone();
        let tx = self.document_state.tx.clone();
        self.document_state.loading = true;
        std::thread::spawn(move || {
            let result = host
                .lists(&project)
                .map_err(|e: anyhow::Error| format!("{e:#}"));
            let _ = tx.send(FileEvent::Lists {
                session: sid,
                result,
            });
        });
        let keys = self
            .document_state
            .tabs
            .iter()
            .filter(|t| !t.historical)
            .map(|t| t.key.clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.read_document(&key);
        }
    }

    pub(in crate::desktop::workspace) fn on_file(
        &mut self,
        event: FileEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        match event {
            FileEvent::WatchReady {
                session,
                signature,
                result,
            } if session == sid && signature == self.document_state.watched_session => match result
            {
                Ok(watcher) => self.document_state.watcher = Some(watcher),
                Err(_) => {
                    self.document_state.watched_session.clear();
                    self.document_state.watch_retry = Some(Instant::now() + Duration::from_secs(5));
                }
            },
            FileEvent::References {
                session,
                request,
                result,
            } if session == sid => self.finish_file_references(request, result, cx),
            FileEvent::Saved {
                session,
                key,
                request,
                disk,
                text,
                result,
            } if session == sid => {
                self.finish_save(&key, &request, (disk, text), result, window, cx)
            }
            FileEvent::Changed {
                session,
                links_changed,
            } if session == sid => {
                if links_changed {
                    self.document_state.file_link_revision =
                        self.document_state.file_link_revision.wrapping_add(1);
                }
                self.document_state
                    .refresh_pending
                    .get_or_insert_with(Instant::now);
            }
            FileEvent::Lists { session, result } if session == sid => {
                self.document_state.loading = false;
                match result {
                    Ok((files, changed)) => {
                        if self.document_state.files != files {
                            let posix = self
                                .session()
                                .is_some_and(|session| session.environment_id() != "local");
                            self.document_state.file_link_revision =
                                self.document_state.file_link_revision.wrapping_add(1);
                            self.document_state.tree.update(cx, |t, cx| {
                                crate::desktop::workspace::documents::file_tree::update_for_host(
                                    t, &files, posix, cx,
                                )
                            });
                            self.document_state.files = files;
                        }
                        if self.document_state.changes != changed {
                            self.document_state.changes = changed;
                        }
                    }
                    Err(e) => self.runtime.error = Some(e),
                }
            }
            FileEvent::Snapshot {
                session,
                request,
                result,
            } if session == sid => self.finish_document(&request, result, window, cx),
            _ => {}
        }
    }
}

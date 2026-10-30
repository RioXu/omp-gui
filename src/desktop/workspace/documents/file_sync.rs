use super::*;
use notify::{EventKind, RecursiveMode, Watcher};

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
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        if sid == self.document_state.watched_session {
            return;
        }
        self.document_state.watcher = None;
        self.document_state.watched_session = sid.clone();
        self.document_state.loading = false;
        self.document_state.refresh_pending = Some(Instant::now());
        let Some(s) = self.session() else {
            return;
        };
        let root = s.cwd().to_owned();
        let watched_root = root.clone();
        let tx = self.document_state.tx.clone();
        if self.demo {
            return;
        }
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let relevant = match event {
                    Ok(e) => {
                        !matches!(e.kind, EventKind::Access(_))
                            && e.paths.iter().any(|p| {
                                p.strip_prefix(&watched_root)
                                    .map_or(true, |p| !ignored_watch_path(p))
                            })
                    }
                    Err(_) => true,
                };
                if relevant {
                    let _ = tx.send(FileEvent::Changed {
                        session: sid.clone(),
                    });
                }
            }) {
                Ok(w) => w,
                Err(_) => return,
            };
        if watcher.watch(&root, RecursiveMode::Recursive).is_ok() {
            self.document_state.watcher = Some(watcher);
        }
    }

    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.document_state.loading {
            return;
        }
        self.document_state.last_scan = Instant::now();
        self.document_state.refresh_pending = None;
        if self.demo {
            if let Some(s) = self.session() {
                self.document_state.changes =
                    s.snapshots.values().map(|s| s.path.clone()).collect();
                self.document_state.changes.sort();
                self.document_state.changes.dedup();
                self.document_state.files = self.document_state.changes.clone();
                self.document_state.tree.update(cx, |t, cx| {
                    crate::desktop::workspace::documents::file_tree::update(
                        t,
                        &self.document_state.files,
                        cx,
                    )
                });
                self.document_state.change_tree.update(cx, |t, cx| {
                    crate::desktop::workspace::documents::file_tree::update(
                        t,
                        &self.document_state.changes,
                        cx,
                    )
                });
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let project = s.cwd().to_owned();
        let sid = s.id.clone();
        let tx = self.document_state.tx.clone();
        self.document_state.loading = true;
        std::thread::spawn(move || {
            let result = (|| {
                Ok((
                    project_files::list(&project, false)?,
                    project_files::list(&project, true)?,
                ))
            })()
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
            FileEvent::Changed { session } if session == sid => {
                self.document_state
                    .refresh_pending
                    .get_or_insert_with(Instant::now);
            }
            FileEvent::Lists { session, result } if session == sid => {
                self.document_state.loading = false;
                match result {
                    Ok((files, changed)) => {
                        if self.document_state.files != files {
                            self.document_state.tree.update(cx, |t, cx| {
                                crate::desktop::workspace::documents::file_tree::update(
                                    t, &files, cx,
                                )
                            });
                            self.document_state.files = files;
                        }
                        if self.document_state.changes != changed {
                            self.document_state.change_tree.update(cx, |t, cx| {
                                crate::desktop::workspace::documents::file_tree::update(
                                    t, &changed, cx,
                                )
                            });
                            self.document_state.changes = changed;
                        }
                    }
                    Err(e) => self.error = Some(e),
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

fn ignored_watch_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("target" | "node_modules" | ".venv")
        )
    })
}

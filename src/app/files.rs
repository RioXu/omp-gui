use super::*;
use notify::{EventKind, RecursiveMode, Watcher};

impl Workspace {
    pub fn clear_file_preview(&mut self) {
        self.preview_request = None;
        self.panel_text = None;
        self.diff = None;
        self.viewed_quote = None;
        self.viewed_path = None;
        self.viewed_snapshot = None;
        self.historical = false;
        self.selected = None;
    }

    pub fn select_panel(&mut self, tab: &'static str) {
        self.panel_tab = tab;
        self.panel_open = true;
    }

    pub fn close_panel(&mut self) {
        self.panel_open = false;
    }

    pub(super) fn watch_files(&mut self) {
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        if sid == self.watched_session {
            return;
        }
        self.file_watcher = None;
        self.watched_session = sid.clone();
        self.loading_files = false;
        self.files_changed_at = Some(Instant::now());
        let Some(s) = self.session() else {
            return;
        };
        let root = s.cwd().to_owned();
        let watched_root = root.clone();
        let tx = self.file_tx.clone();
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
            self.file_watcher = Some(watcher);
        }
    }

    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.loading_files {
            return;
        }
        self.last_files_scan = Instant::now();
        self.files_changed_at = None;
        if self.demo {
            if let Some(s) = self.session() {
                self.changed = s.snapshots.values().map(|s| s.path.clone()).collect();
                self.changed.sort();
                self.changed.dedup();
                self.files = self.changed.clone();
                self.file_tree
                    .update(cx, |t, cx| crate::file_tree::update(t, &self.files, cx));
                self.change_tree
                    .update(cx, |t, cx| crate::file_tree::update(t, &self.changed, cx));
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let project = s.cwd().to_owned();
        let sid = s.id.clone();
        let tx = self.file_tx.clone();
        self.loading_files = true;
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
            .documents
            .iter()
            .filter(|t| !t.historical)
            .map(|t| t.key.clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.read_document(&key);
        }
    }

    pub fn on_file(&mut self, event: FileEvent, window: &mut Window, cx: &mut Context<Self>) {
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        match event {
            FileEvent::Changed { session } if session == sid => {
                self.files_changed_at.get_or_insert_with(Instant::now);
            }
            FileEvent::Lists { session, result } if session == sid => {
                self.loading_files = false;
                match result {
                    Ok((files, changed)) => {
                        if self.files != files {
                            self.file_tree
                                .update(cx, |t, cx| crate::file_tree::update(t, &files, cx));
                            self.files = files;
                        }
                        if self.changed != changed {
                            self.change_tree
                                .update(cx, |t, cx| crate::file_tree::update(t, &changed, cx));
                            self.changed = changed;
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

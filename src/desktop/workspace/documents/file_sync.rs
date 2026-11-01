use super::*;
use crate::infrastructure::filesystem::changes::{ChangeKind, ChangedFile};
use notify::{EventKind, RecursiveMode, Watcher};
use omp_core::{
    environment::HostPath,
    host::{LocalHost, ProjectFiles},
};

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
        let external: std::collections::BTreeSet<_> = self
            .document_state
            .tabs
            .iter()
            .filter(|t| !t.historical && t.path.is_absolute())
            .filter_map(|t| t.path.parent().map(std::path::Path::to_owned))
            .collect();
        let signature = format!(
            "{sid}|{}",
            external
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join("|")
        );
        if signature == self.document_state.watched_session {
            return;
        }
        self.document_state.watcher = None;
        self.document_state.watched_session = signature;
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
                let links_changed = match &event {
                    Ok(e) => matches!(
                        e.kind,
                        EventKind::Create(_)
                            | EventKind::Remove(_)
                            | EventKind::Modify(notify::event::ModifyKind::Name(_))
                    ),
                    Err(_) => true,
                };
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
                        links_changed,
                    });
                }
            }) {
                Ok(w) => w,
                Err(_) => return,
            };
        if watcher.watch(&root, RecursiveMode::Recursive).is_ok() {
            for parent in external {
                let _ = watcher.watch(&parent, RecursiveMode::NonRecursive);
            }
            self.document_state.watcher = Some(watcher);
        }
    }

    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.polling_background {
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
        let project = s.cwd().to_owned();
        let sid = s.id.clone();
        let tx = self.document_state.tx.clone();
        self.document_state.loading = true;
        std::thread::spawn(move || {
            let result = LocalHost
                .lists(&HostPath::Local(project))
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
                            self.document_state.file_link_revision =
                                self.document_state.file_link_revision.wrapping_add(1);
                            self.document_state.tree.update(cx, |t, cx| {
                                crate::desktop::workspace::documents::file_tree::update(
                                    t, &files, cx,
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

fn ignored_watch_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("target" | "node_modules" | ".venv")
        )
    })
}

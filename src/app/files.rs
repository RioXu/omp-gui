use super::*;

impl Workspace {
    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.loading_files {
            return;
        }
        if self.demo {
            if let Some(s) = self.session() {
                self.changed = s.snapshots.values().map(|s| s.path.clone()).collect();
                self.changed.sort();
                self.changed.dedup();
                self.files = self.changed.clone();
                self.file_tree.update(cx, |t, cx| {
                    t.set_items(crate::file_tree::items(&self.files), cx)
                });
                self.change_tree.update(cx, |t, cx| {
                    t.set_items(crate::file_tree::items(&self.changed), cx)
                });
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
            for changed in [true, false] {
                let result = project_files::list(&project, changed).map_err(|e| format!("{e:#}"));
                let _ = tx.send(FileEvent::List {
                    session: sid.clone(),
                    changed,
                    result,
                });
            }
        });
    }
    pub fn open_file(&mut self, path: PathBuf) {
        if self.demo {
            let snapshot = self
                .session()
                .and_then(|s| s.snapshots.values().find(|n| n.path == path))
                .cloned();
            if let Some(snapshot) = snapshot {
                let _ = self.file_tx.send(FileEvent::Snapshot {
                    session: self.session().unwrap().id.clone(),
                    result: Ok(snapshot),
                });
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let sid = s.id.clone();
        let root = s.cwd().to_owned();
        let tx = self.file_tx.clone();
        self.status = "正在读取文件…".into();
        std::thread::spawn(move || {
            let result = project_files::snapshot(&root, &path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(FileEvent::Snapshot {
                session: sid,
                result,
            });
        });
    }
    pub fn on_file(&mut self, event: FileEvent, cx: &mut Context<Self>) {
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        match event {
            FileEvent::List {
                session,
                changed,
                result,
            } if session == sid => {
                self.loading_files = false;
                match result {
                    Ok(paths) => {
                        let tree = if changed {
                            self.change_tree.clone()
                        } else {
                            self.file_tree.clone()
                        };
                        let current = if changed { &self.changed } else { &self.files };
                        if current != &paths {
                            tree.update(cx, |t, cx| {
                                t.set_items(crate::file_tree::items(&paths), cx)
                            });
                        }
                        if changed {
                            self.changed = paths
                        } else {
                            self.files = paths
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            FileEvent::Snapshot { session, result } if session == sid => match result {
                Ok(snapshot) => {
                    self.viewed_quote = None;
                    self.diff = if self.panel_tab == "变更" && !snapshot.patch.is_empty() {
                        DiffFile::parse(&snapshot.patch)
                            .ok()
                            .map(|files| cx.new(|cx| DiffState::new(files, cx)))
                    } else {
                        None
                    };
                    self.selected = None;
                    self.viewed_path = Some(snapshot.path.clone());
                    self.viewed_snapshot = Some(snapshot.id.clone());
                    self.historical = false;
                    let body = if self.panel_tab == "文件" {
                        fenced(&snapshot.text, "")
                    } else {
                        snapshot.document()
                    };
                    let source = Source::File {
                        path: snapshot.path.clone(),
                        snapshot: snapshot.id.clone(),
                    };
                    self.panel_text = Some(TextEntry {
                        state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true)),
                        body,
                        source,
                    });
                    if let Some(entry) = &self.panel_text {
                        highlight_diff(entry, cx);
                    }
                    if let Some(s) = self.session_mut() {
                        s.snapshots.insert(snapshot.id.clone(), snapshot);
                    }
                    self.dirty = true;
                    self.status = "文件已载入".into();
                }
                Err(e) => self.error = Some(e),
            },
            FileEvent::Current {
                session,
                path,
                result,
            } if session == sid => match result {
                Ok(text) => {
                    self.viewed_quote = None;
                    self.diff = None;
                    let body = fenced(&text, "");
                    self.viewed_path = Some(path.clone());
                    self.historical = false;
                    self.panel_text = Some(TextEntry {
                        state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(false)),
                        body,
                        source: Source::File {
                            path,
                            snapshot: String::new(),
                        },
                    });
                }
                Err(e) => self.error = Some(e),
            },
            _ => {}
        }
    }
}

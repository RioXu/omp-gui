use super::*;
use crate::domain::file_buffer::FileBuffer;
use omp_core::{environment::HostPath, host::ProjectFiles};

#[derive(Clone)]
pub(crate) struct DocumentTab {
    pub key: String,
    pub path: PathBuf,
    pub changes: bool,
    pub historical: bool,
    pub request: Option<String>,
    pub snapshot: Option<Snapshot>,
    pub entry: Option<TextEntry>,
    pub editor: Option<Entity<EditorState>>,
    pub diff: Option<Entity<DiffState>>,
    pub quote: Option<String>,
    pub error: Option<String>,
    pub buffer: Option<FileBuffer>,
    pub save_path: Option<HostPath>,
    pub dirty: bool,
    pub saving: Option<String>,
    pub close_after_save: Option<String>,
}

impl Workspace {
    pub fn activate_document(&mut self, key: Option<String>) {
        self.clear_file_preview();
        self.document_state.active = key;
        if let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref())
        {
            self.document_state.text = tab.entry.clone();
            self.document_state.diff = tab.diff.clone();
            self.document_state.historical = tab.historical;
            self.document_state.path = Some(tab.path.clone());
            self.document_state.snapshot = tab.snapshot.as_ref().map(|s| s.id.clone());
            self.document_state.quote = tab.quote.clone();
        }
    }

    pub fn close_document(&mut self, key: &str) {
        let Some(ix) = self.document_state.tabs.iter().position(|t| t.key == key) else {
            return;
        };
        self.document_state.tabs.remove(ix);
        self.document_state.subscriptions.remove(key);
        if self.document_state.active.as_deref() == Some(key) {
            let next = ix
                .checked_sub(1)
                .and_then(|i| self.document_state.tabs.get(i))
                .or_else(|| self.document_state.tabs.first())
                .map(|t| t.key.clone());
            self.activate_document(next);
        }
    }

    pub fn open_file(&mut self, path: PathBuf) {
        self.open_document(path, self.panel_tab == "变更");
    }

    pub fn open_document(&mut self, path: PathBuf, changes: bool) {
        let path = self
            .session()
            .filter(|s| s.environment_id() == "local")
            .map(|s| {
                crate::infrastructure::filesystem::file_references::document_path(s.cwd(), &path)
            })
            .unwrap_or(path);
        let path_identity = if self
            .session()
            .is_some_and(|s| s.environment_id() != "local")
        {
            path.to_string_lossy().into_owned()
        } else {
            path.to_string_lossy().replace('\\', "/")
        };
        let key = format!(
            "{}:{}",
            if changes { "diff" } else { "file" },
            path_identity
        );
        if !self.document_state.tabs.iter().any(|t| t.key == key) {
            self.document_state.tabs.push(DocumentTab {
                key: key.clone(),
                path,
                changes,
                historical: false,
                request: None,
                snapshot: None,
                entry: None,
                editor: None,
                diff: None,
                quote: None,
                error: None,
                buffer: None,
                save_path: None,
                dirty: false,
                saving: None,
                close_after_save: None,
            });
            self.read_document(&key);
        }
        self.activate_document(Some(key));
    }

    pub(in crate::desktop::workspace) fn read_document(&mut self, key: &str) {
        let Some(s) = self.session() else {
            return;
        };
        let sid = s.id.clone();
        let root = s.host_cwd();
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(error) => {
                self.runtime.error = Some(format!("{error:#}"));
                return;
            }
        };
        let demo_snapshot = self
            .document_state
            .tabs
            .iter()
            .find(|t| t.key == key)
            .and_then(|t| s.snapshots.values().find(|s| s.path == t.path))
            .cloned();
        let Some(tab) = self
            .document_state
            .tabs
            .iter_mut()
            .find(|t| t.key == key && !t.historical && !t.dirty && t.request.is_none())
        else {
            return;
        };
        let request = id();
        tab.request = Some(request.clone());
        let path = tab.path.clone();
        let changes = tab.changes;
        let tx = self.document_state.tx.clone();
        if self.demo {
            let _ = tx.send(FileEvent::Snapshot {
                session: sid,
                request,
                result: demo_snapshot
                    .map(|snapshot| LoadedDocument {
                        snapshot,
                        target: None,
                    })
                    .ok_or_else(|| "文件不存在".into()),
            });
            return;
        }
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let (snapshot, target) = host.document(&root, &path, changes)?;
                Ok(LoadedDocument { snapshot, target })
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(FileEvent::Snapshot {
                session: sid,
                request,
                result,
            });
        });
    }

    pub(in crate::desktop::workspace) fn finish_document(
        &mut self,
        request: &str,
        result: Result<LoadedDocument, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self
            .document_state
            .tabs
            .iter()
            .position(|t| t.request.as_deref() == Some(request))
        else {
            return;
        };
        self.document_state.tabs[ix].request = None;
        if self.document_state.tabs[ix].is_dirty(cx) {
            return;
        }
        let LoadedDocument { snapshot, target } = match result {
            Ok(loaded) => loaded,
            Err(error) => {
                self.document_state.tabs[ix].error = Some(error);
                return;
            }
        };
        let tab = &mut self.document_state.tabs[ix];
        let is_new = tab.editor.is_none();
        tab.error = None;
        if tab
            .snapshot
            .as_ref()
            .is_some_and(|s| s.text == snapshot.text && s.patch == snapshot.patch)
            && tab
                .buffer
                .as_ref()
                .is_none_or(|buffer| buffer.disk_text() == snapshot.text)
        {
            return;
        }
        let language = language_for_path(&tab.path);
        let buffer = FileBuffer::new(&snapshot.text);
        let editor_text = buffer.editor_text().to_owned();
        let unchanged = tab
            .buffer
            .as_ref()
            .is_some_and(|b| b.disk_text() == snapshot.text);
        if let Some(editor) = &tab.editor {
            if !unchanged {
                // Preserve the control and scroll position when the on-disk text changes.
                editor.update(cx, |e, cx| {
                    let scroll = e.scroll_offset();
                    let selection = e.selected_range();
                    e.set_value(editor_text.clone(), window, cx);
                    let len = editor_text.len();
                    let boundary = |mut i: usize| {
                        i = i.min(len);
                        while !editor_text.is_char_boundary(i) {
                            i -= 1;
                        }
                        i
                    };
                    e.set_selected_range(boundary(selection.start)..boundary(selection.end), cx);
                    e.set_scroll_offset(scroll, cx);
                });
            }
        } else {
            tab.editor = Some(cx.new(|cx| {
                EditorState::new(window, cx)
                    .language(language)
                    .line_number(true)
                    .default_value(editor_text.clone())
            }));
        }
        if !tab.changes && target.is_some() {
            tab.buffer = Some(buffer);
            tab.save_path = target;
            if !self.document_state.subscriptions.contains_key(&tab.key) {
                let key = tab.key.clone();
                let editor = tab.editor.clone().unwrap();
                let subscription = cx.subscribe(&editor, move |this, editor, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.editor_changed(&key, editor, cx);
                    }
                });
                self.document_state
                    .subscriptions
                    .insert(tab.key.clone(), subscription);
            }
        }
        tab.dirty = false;
        tab.diff = if tab.changes && !snapshot.patch.is_empty() {
            DiffFile::parse(&snapshot.patch)
                .ok()
                .filter(|f| !f.is_empty())
                .map(|f| {
                    cx.new(|cx| {
                        DiffState::new(searchable_diff(f), cx)
                            .with_syntax_max_line_length(2 * 1024 * 1024)
                    })
                })
        } else {
            None
        };
        let body = if tab.changes {
            snapshot.document()
        } else {
            fenced(&editor_text, language)
        };
        tab.entry = Some(TextEntry {
            offsets: Default::default(),
            state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true)),
            body,
            source: Source::File {
                path: tab.path.clone(),
                snapshot: snapshot.id.clone(),
            },
        });
        tab.snapshot = Some(snapshot.clone());
        let active = self.document_state.active.clone();
        if active.as_deref() == Some(&self.document_state.tabs[ix].key) {
            self.activate_document(active);
        }
        if let Some(s) = self.session_mut() {
            s.snapshots.insert(snapshot.id.clone(), snapshot);
        }
        self.dirty = true;
        if is_new
            && self.document_state.active.as_deref() == Some(&self.document_state.tabs[ix].key)
        {
            self.focus_document_editor(window, cx);
        }
    }

    pub(in crate::desktop::workspace) fn retain_historical_document(&mut self) {
        let key = format!(
            "history:{}",
            self.document_state.snapshot.as_deref().unwrap_or("quote")
        );
        self.document_state.tabs.retain(|t| t.key != key);
        self.document_state.tabs.push(DocumentTab {
            key: key.clone(),
            path: self
                .document_state
                .path
                .clone()
                .unwrap_or_else(|| "历史引用".into()),
            changes: true,
            historical: true,
            request: None,
            snapshot: None,
            entry: self.document_state.text.clone(),
            editor: None,
            diff: self.document_state.diff.clone(),
            quote: self.document_state.quote.clone(),
            error: None,
            buffer: None,
            save_path: None,
            dirty: false,
            saving: None,
            close_after_save: None,
        });
        self.document_state.active = Some(key);
    }
}

pub(in crate::desktop::workspace) fn searchable_diff(files: Vec<DiffFile>) -> Vec<DiffFile> {
    // Native Diff skips all application highlights for its special "text"
    // language. A plain language still has no syntax, but accepts search marks.
    files
        .into_iter()
        .map(|file| {
            if file.language().as_str() == "text" {
                file.with_language("plain")
            } else {
                file
            }
        })
        .collect()
}

pub(crate) fn language_for_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "html" => "html",
        "css" => "css",
        "c" | "h" => "c",
        "cpp" | "hpp" => "cpp",
        "go" => "go",
        "sh" => "bash",
        _ => "plain",
    }
}

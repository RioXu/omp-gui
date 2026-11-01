use super::*;
use crate::domain::file_buffer::FileBuffer;
use omp_core::{
    environment::HostPath,
    host::{LocalHost, ProjectFiles},
};

impl DocumentTab {
    pub(in crate::desktop::workspace) fn is_dirty(&self, cx: &App) -> bool {
        self.buffer
            .as_ref()
            .zip(self.editor.as_ref())
            .is_some_and(|(buffer, editor)| buffer.is_dirty(&editor.read(cx).value()))
    }
}

impl Workspace {
    pub(in crate::desktop::workspace) fn has_dirty_documents(&self, cx: &App) -> bool {
        self.document_state.tabs.iter().any(|tab| tab.is_dirty(cx))
    }

    pub(in crate::desktop::workspace) fn editor_changed(
        &mut self,
        key: &str,
        editor: Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.document_state.tabs.iter_mut().find(|t| {
            t.key == key
                && t.editor
                    .as_ref()
                    .is_some_and(|e| e.entity_id() == editor.entity_id())
        }) else {
            return;
        };
        let was_dirty = tab.dirty;
        tab.dirty = tab
            .buffer
            .as_ref()
            .is_some_and(|buffer| buffer.is_dirty(&editor.read(cx).value()));
        if was_dirty && !tab.dirty {
            self.document_state.refresh_pending = Some(Instant::now());
        }
        if self.search_state.open && self.document_state.active.as_deref() == Some(key) {
            self.update_search(false, cx);
        }
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn save_active_document(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.document_state.active.clone() {
            self.save_document(&key, window, cx);
        }
    }

    pub(in crate::desktop::workspace) fn focus_document_editor(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(editor) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref() && t.buffer.is_some())
            .and_then(|t| t.editor.as_ref())
        {
            editor.focus_handle(cx).focus(window, cx);
        }
    }

    pub(in crate::desktop::workspace) fn save_document(
        &mut self,
        key: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.save_disabled {
            return false;
        }
        let Some(root) = self.session().map(|s| s.cwd().to_owned()) else {
            return false;
        };
        let Some(tab) = self.document_state.tabs.iter_mut().find(|t| t.key == key) else {
            return false;
        };
        let Some((buffer, editor, target)) = tab
            .buffer
            .as_mut()
            .zip(tab.editor.as_ref())
            .zip(tab.save_path.as_ref())
            .map(|((b, e), p)| (b, e, p))
        else {
            return false;
        };
        let text = editor.read(cx).value().to_string();
        if !buffer.is_dirty(&text) {
            return true;
        }
        let disk = buffer.encode(&text);
        if let Err(error) = LocalHost.save(
            &HostPath::Local(root),
            &tab.path,
            &HostPath::Local(target.clone()),
            buffer.disk_text(),
            &disk,
        ) {
            tab.error = Some(format!("保存失败：{error:#}"));
            cx.notify();
            return false;
        }
        buffer.saved(disk, text);
        tab.dirty = false;
        tab.error = None;
        // Ignore a read queued before this save; the new refresh reads its revision.
        tab.request = None;
        self.sync_editor_snapshot(key, cx);
        self.refresh_files(cx);
        cx.notify();
        true
    }

    pub(in crate::desktop::workspace) fn sync_selected_editor_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let key = self.document_state.active.clone().filter(|key| {
            self.document_state
                .tabs
                .iter()
                .find(|t| &t.key == key)
                .and_then(|t| t.editor.as_ref())
                .is_some_and(|e| !e.read(cx).selected_range().is_empty())
        });
        if let Some(key) = key {
            self.sync_editor_snapshot(&key, cx);
        }
    }

    fn sync_editor_snapshot(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(tab) = self
            .document_state
            .tabs
            .iter_mut()
            .find(|t| t.key == key && !t.changes && !t.historical)
        else {
            return;
        };
        let Some(editor) = &tab.editor else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        let snapshot_text = tab
            .buffer
            .as_ref()
            .map(|buffer| buffer.encode(&text))
            .unwrap_or(text);
        if tab
            .snapshot
            .as_ref()
            .is_some_and(|s| s.text == snapshot_text)
        {
            return;
        }
        let before = tab
            .snapshot
            .as_ref()
            .map(|s| s.before.clone())
            .unwrap_or_default();
        let snapshot = Snapshot::new(tab.path.clone(), before, snapshot_text);
        let body = fenced(
            &FileBuffer::decode(&snapshot.text),
            language_for_path(&tab.path),
        );
        let entry = TextEntry {
            offsets: Default::default(),
            state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true)),
            body,
            source: Source::File {
                path: tab.path.clone(),
                snapshot: snapshot.id.clone(),
            },
        };
        tab.entry = Some(entry.clone());
        tab.snapshot = Some(snapshot.clone());
        if self.document_state.active.as_deref() == Some(key) {
            self.document_state.text = Some(entry);
            self.document_state.snapshot = Some(snapshot.id.clone());
        }
        if let Some(session) = self.session_mut() {
            session.snapshots.insert(snapshot.id.clone(), snapshot);
        }
        self.dirty = true;
    }
}

//! Workspace coordinator: persisted conversation data and native feature state.
mod composer;
mod conversation;
pub(in crate::desktop::workspace) mod documents;
mod initialization;
mod preferences;
pub(in crate::desktop::workspace) mod runtime;
mod search;
mod view;
use composer::ComposerState;
use conversation::demo::demo_session;
use conversation::submission::{PendingSubmission, SubmittedInput};
use documents::DocumentState;
use search::SearchState;

use crate::domain::Anchor;
use crate::{
    domain::*,
    infrastructure::{
        filesystem::project_files,
        omp::{Client as OmpClient, Event},
        persistence::workspace_store::{self as storage, Persistence},
    },
};
use gpui_kit::component::{
    ActiveTheme, Theme, ThemeMode, WindowExt,
    diff::{DiffFile, DiffLineRange, DiffSide, DiffState},
    input::{EditorState, InputEvent, InputState, TextareaState},
    text::{RangeHighlight, TextViewState},
    tree::TreeState,
};
use gpui_kit::*;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

actions!(
    omp_gui,
    [
        SendPrompt,
        AddComment,
        TogglePanel,
        NewSession,
        OpenProject,
        FindInSession,
        AcceptCompletion,
        NextCompletion,
        PreviousCompletion,
        DismissCompletion,
        NextSearchMatch,
        PreviousSearchMatch,
        CloseSearch,
        SaveDocument
    ]
);

#[derive(Clone)]
pub(crate) struct TextEntry {
    pub state: Entity<TextViewState>,
    pub body: String,
    pub source: Source,
    pub offsets: crate::desktop::markdown::SourceOffsets,
}

pub(crate) struct Workspace {
    runtime: runtime::SessionRuntime,
    background: HashMap<String, runtime::SessionRuntime>,
    polling_background: bool,
    attachment_imports: composer::AttachmentImports,
    document_state: DocumentState,
    search_state: SearchState,
    session_sync: conversation::session_sync::SessionSync,
    selection_dirty: bool,
    settings_view: Option<Entity<crate::desktop::settings::SettingsView>>,
    show_archived: bool,
    collapsed_projects: std::collections::HashSet<PathBuf>,
    db: Database,
    active: Option<usize>,
    persistence: Persistence,
    save_disabled: bool,
    comment_input: Entity<TextareaState>,
    focus: FocusHandle,
    panel_open: bool,
    panel_tab: &'static str,
    selected: Option<Anchor>,
    selection_point: Point<Pixels>,
    comment_editor: Option<Anchor>,
    editing_comment: Option<String>,
    pending_open: bool,
    demo: bool,
    last_save: Instant,
    dirty: bool,
    _subscriptions: Vec<Subscription>,
    _data_lock: Option<std::fs::File>,
}
impl Workspace {
    pub fn session(&self) -> Option<&Session> {
        self.active.and_then(|i| self.db.sessions.get(i))
    }
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.active.and_then(|i| self.db.sessions.get_mut(i))
    }
    pub fn persist(&mut self) {
        if self.save_disabled {
            return;
        }
        if let Err(e) = self.persistence.save(&self.db) {
            self.runtime.error = Some(e.to_string());
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.persist();
    }
}

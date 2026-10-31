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
use runtime::ModelControlState;
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
        CloseSearch
    ]
);

#[derive(Clone)]
pub(crate) struct TextEntry {
    pub state: Entity<TextViewState>,
    pub body: String,
    pub source: Source,
}

pub(crate) struct Workspace {
    model_controls: ModelControlState,
    document_state: DocumentState,
    search_state: SearchState,
    composer_state: ComposerState,
    session_sync: conversation::session_sync::SessionSync,
    transcript: Vec<TranscriptRow>,
    tool_diffs: HashMap<String, Entity<DiffState>>,
    stopping: bool,
    interrupted_run: bool,
    run_message_start: usize,
    submitted_inputs: Vec<SubmittedInput>,
    expanded_process: std::collections::HashSet<String>,
    selection_dirty: bool,
    text_update_pending: bool,
    settings_view: Option<Entity<crate::desktop::settings::SettingsView>>,
    show_archived: bool,
    db: Database,
    active: Option<usize>,
    persistence: Persistence,
    save_disabled: bool,
    comment_input: Entity<TextareaState>,
    answer_input: Entity<InputState>,
    focus: FocusHandle,
    texts: HashMap<String, TextEntry>,
    panel_open: bool,
    panel_tab: &'static str,
    engine: Option<OmpClient>,
    connected: bool,
    running: bool,
    starting: bool,
    status: String,
    error: Option<String>,
    todos: Vec<Value>,
    agents: Vec<Value>,
    context_percent: f64,
    context_limit: u64,
    active_tools: HashMap<String, String>,
    requests: VecDeque<Value>,
    selected: Option<Anchor>,
    selection_point: Point<Pixels>,
    comment_editor: Option<Anchor>,
    editing_comment: Option<String>,
    pending_open: bool,
    follow_up: Option<String>,
    in_flight_comments: Vec<String>,
    rpc_messages: HashMap<String, String>,
    pending_send: Option<PendingSubmission>,
    queued_inputs: Vec<String>,
    steered_inputs: Vec<String>,
    steering_request: Option<String>,
    list: ListState,
    demo: bool,
    last_state: Instant,
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
            self.error = Some(e.to_string());
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

mod demo;
mod init;
mod theme;
use demo::demo_session;
pub(crate) use theme::apply_accent;
mod comments;
mod completion;
mod connection;
mod document_search;
pub(crate) mod documents;
mod events;
mod files;
pub(crate) mod models;
mod poll;
mod preferences;
mod queue;
mod search;
mod sessions;
mod settings_commands;
mod submission;
mod transcript;
use submission::{PendingSubmission, SubmittedInput};

use crate::domain::Anchor;
use crate::{
    domain::*,
    engine::{Engine, Event},
    files as project_files,
    storage::{self, Persistence},
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
pub(crate) enum FileEvent {
    Changed {
        session: String,
    },
    Lists {
        session: String,
        result: Result<(Vec<PathBuf>, Vec<PathBuf>), String>,
    },
    Snapshot {
        session: String,
        request: String,
        result: Result<Snapshot, String>,
    },
}

pub(crate) struct Workspace {
    pub documents: Vec<documents::DocumentTab>,
    pub active_document: Option<String>,
    pub file_watcher: Option<notify::RecommendedWatcher>,
    pub watched_session: String,
    pub files_changed_at: Option<Instant>,
    pub last_files_scan: Instant,
    pub search_input: Entity<InputState>,
    pub search_open: bool,
    pub search_scope: Option<String>,
    pub search_source: String,
    pub diff_search_positions: Vec<gpui_kit::component::diff::DiffLinePosition>,
    pub search_hits: Vec<(String, std::ops::Range<usize>)>,
    pub search_index: usize,
    pub search_documents: HashMap<String, gpui_kit::component::text::RenderedText>,
    pub slash_commands: Vec<Value>,
    pub completion_index: usize,
    pub completion_dismissed: bool,
    pub transcript: Vec<TranscriptRow>,
    pub tool_diffs: HashMap<String, Entity<DiffState>>,
    pub stopping: bool,
    pub interrupted_run: bool,
    pub run_message_start: usize,
    pub submitted_inputs: Vec<SubmittedInput>,
    pub expanded_process: std::collections::HashSet<String>,
    pub file_tree: Entity<TreeState>,
    pub change_tree: Entity<TreeState>,
    pub selection_dirty: bool,
    pub text_update_pending: bool,
    pub settings_view: Option<Entity<crate::settings_ui::SettingsView>>,
    pub show_archived: bool,
    pub diff: Option<Entity<DiffState>>,
    pub viewed_quote: Option<String>,
    pub db: Database,
    pub active: Option<usize>,
    pub persistence: Persistence,
    pub save_disabled: bool,
    pub input: Entity<TextareaState>,
    pub comment_input: Entity<TextareaState>,
    pub answer_input: Entity<InputState>,
    pub focus: FocusHandle,
    pub texts: HashMap<String, TextEntry>,
    pub panel_text: Option<TextEntry>,
    pub preview_request: Option<String>,
    pub panel_open: bool,
    pub panel_tab: &'static str,
    pub files: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
    pub file_tx: mpsc::Sender<FileEvent>,
    pub file_rx: mpsc::Receiver<FileEvent>,
    pub viewed_snapshot: Option<String>,
    pub viewed_path: Option<PathBuf>,
    pub historical: bool,
    pub loading_files: bool,
    pub engine: Option<Engine>,
    pub connected: bool,
    pub running: bool,
    pub starting: bool,
    pub status: String,
    pub error: Option<String>,
    pub models: Vec<Value>,
    pub current_model: Value,
    pub thinking_levels: Vec<String>,
    pub thinking_levels_request: Option<String>,
    pub thinking_auto: bool,
    pub thinking_entries_cursor: Option<String>,
    pub pending_model_control: Option<(String, Option<String>)>,
    pub model_label: String,
    pub thinking: String,
    pub todos: Vec<Value>,
    pub agents: Vec<Value>,
    pub context_percent: f64,
    pub context_limit: u64,
    pub active_tools: HashMap<String, String>,
    pub requests: VecDeque<Value>,
    pub selected: Option<Anchor>,
    pub selection_point: Point<Pixels>,
    pub comment_editor: Option<Anchor>,
    pub editing_comment: Option<String>,
    pub pending_open: bool,
    pub follow_up: Option<String>,
    pub in_flight_comments: Vec<String>,
    pub rpc_messages: HashMap<String, String>,
    pub pending_send: Option<PendingSubmission>,
    pub queued_inputs: Vec<String>,
    pub steered_inputs: Vec<String>,
    pub steering_request: Option<String>,
    pub list: ListState,
    pub demo: bool,
    pub last_state: Instant,
    pub last_save: Instant,
    pub dirty: bool,
    pub _subscriptions: Vec<Subscription>,
    pub _data_lock: Option<std::fs::File>,
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
    pub fn sync_texts(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        let messages = &self.db.sessions[ix].messages;
        for m in messages {
            if let Some(entry) = self.texts.get_mut(&m.id) {
                if entry.body != m.text {
                    entry.body = m.text.clone();
                    entry.state.update(cx, |s, cx| s.set_text(&m.text, cx));
                }
            } else {
                let state = cx.new(|cx| {
                    TextViewState::markdown(&m.text, cx).selectable(m.role == "assistant")
                });
                self.texts.insert(
                    m.id.clone(),
                    TextEntry {
                        state,
                        body: m.text.clone(),
                        source: Source::Message(m.id.clone()),
                    },
                );
            }
        }
        self.transcript = transcript_rows(messages);
        self.refresh_transcript();
        if self.search_open {
            self.update_search(false, cx);
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.persist();
    }
}

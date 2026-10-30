mod demo;
mod init;
mod theme;
use demo::demo_session;
pub(crate) use theme::apply_accent;
mod comments;
mod connection;
mod events;
mod files;
mod poll;
mod preferences;
mod queue;
mod sessions;
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
    input::{InputEvent, InputState, TextareaState},
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
    [SendPrompt, AddComment, TogglePanel, NewSession, OpenProject]
);

pub(crate) struct TextEntry {
    pub state: Entity<TextViewState>,
    pub body: String,
    pub source: Source,
}
pub(crate) enum FileEvent {
    List {
        session: String,
        changed: bool,
        result: Result<Vec<PathBuf>, String>,
    },
    Snapshot {
        session: String,
        request: String,
        result: Result<Snapshot, String>,
    },
    Current {
        session: String,
        request: String,
        path: PathBuf,
        result: Result<String, String>,
    },
}

pub(crate) struct Workspace {
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
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.persist();
    }
}

fn highlight_diff(entry: &TextEntry, cx: &mut App) {
    if !entry
        .body
        .lines()
        .next()
        .is_some_and(|l| l.ends_with("diff"))
    {
        return;
    }
    let added = cx.theme().success.opacity(0.14);
    let removed = cx.theme().danger.opacity(0.12);
    entry.state.update(cx, |s, cx| {
        let rendered = s.rendered_text();
        let mut offset = 0;
        let mut highlights = Vec::new();
        for line in entry.body.split_inclusive('\n') {
            let color = if line.starts_with('+') && !line.starts_with("+++") {
                Some(added)
            } else if line.starts_with('-') && !line.starts_with("---") {
                Some(removed)
            } else {
                None
            };
            if let Some(color) = color
                && let Some(range) =
                    rendered.range_for_source(offset..offset + line.trim_end_matches('\n').len())
            {
                highlights.push(RangeHighlight::new(range, color));
            }
            offset += line.len();
        }
        let _ = s.set_range_highlights(highlights, cx);
    });
}

//! Open tabs, file watching, asynchronous reads and active document presentation.
use super::DocumentTab;
use crate::desktop::workspace::TextEntry;
use crate::domain::Snapshot;
use crate::infrastructure::filesystem::changes::ChangedFile;
use gpui_kit::{
    Entity,
    component::{diff::DiffState, tree::TreeState},
};
use std::{path::PathBuf, sync::mpsc, time::Instant};

pub(in crate::desktop::workspace) struct DocumentState {
    pub(in crate::desktop::workspace) tabs: Vec<DocumentTab>,
    pub(in crate::desktop::workspace) active: Option<String>,
    pub(in crate::desktop::workspace) watcher: Option<notify::RecommendedWatcher>,
    pub(in crate::desktop::workspace) watched_session: String,
    pub(in crate::desktop::workspace) refresh_pending: Option<Instant>,
    pub(in crate::desktop::workspace) last_scan: Instant,
    pub(in crate::desktop::workspace) tree: Entity<TreeState>,
    pub(in crate::desktop::workspace) diff: Option<Entity<DiffState>>,
    pub(in crate::desktop::workspace) quote: Option<String>,
    pub(in crate::desktop::workspace) text: Option<TextEntry>,
    pub(in crate::desktop::workspace) files: Vec<PathBuf>,
    pub(in crate::desktop::workspace) changes: Vec<ChangedFile>,
    pub(in crate::desktop::workspace) tx: mpsc::Sender<FileEvent>,
    pub(in crate::desktop::workspace) rx: mpsc::Receiver<FileEvent>,
    pub(in crate::desktop::workspace) snapshot: Option<String>,
    pub(in crate::desktop::workspace) path: Option<PathBuf>,
    pub(in crate::desktop::workspace) historical: bool,
    pub(in crate::desktop::workspace) loading: bool,
}

pub(in crate::desktop::workspace) enum FileEvent {
    Changed {
        session: String,
    },
    Lists {
        session: String,
        result: Result<(Vec<PathBuf>, Vec<ChangedFile>), String>,
    },
    Snapshot {
        session: String,
        request: String,
        result: Result<Snapshot, String>,
    },
}

//! Draft input and command-completion presentation state.
use gpui_kit::{Entity, component::input::TextareaState};
use serde_json::Value;

pub(in crate::desktop::workspace) struct ComposerState {
    pub(in crate::desktop::workspace) input: Entity<TextareaState>,
    pub(in crate::desktop::workspace) commands: Vec<Value>,
    pub(in crate::desktop::workspace) selected_completion: usize,
    pub(in crate::desktop::workspace) dismissed: bool,
    pub(in crate::desktop::workspace) history: Vec<String>,
    pub(in crate::desktop::workspace) history_cursor: Option<usize>,
}

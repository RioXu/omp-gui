//! Search query, matches and rendered-text coordinates for the active tab.
use gpui_kit::{
    Entity,
    component::{diff::DiffLinePosition, input::InputState, text::RenderedText},
};
use std::collections::HashMap;

pub(in crate::desktop::workspace) struct ThinkingSearch {
    pub message: String,
    pub body: String,
    pub state: Entity<gpui_kit::component::text::TextViewState>,
}

pub(in crate::desktop::workspace) struct SearchState {
    pub(in crate::desktop::workspace) input: Entity<InputState>,
    pub(in crate::desktop::workspace) open: bool,
    pub(in crate::desktop::workspace) scope: Option<String>,
    pub(in crate::desktop::workspace) source: String,
    pub(in crate::desktop::workspace) diff_positions: Vec<DiffLinePosition>,
    pub(in crate::desktop::workspace) hits: Vec<(String, std::ops::Range<usize>)>,
    pub(in crate::desktop::workspace) index: usize,
    pub(in crate::desktop::workspace) rendered_text: HashMap<String, RenderedText>,
    pub(in crate::desktop::workspace) thinking: HashMap<String, ThinkingSearch>,
    pub(in crate::desktop::workspace) pending_thinking_reveal: Option<HashMap<String, String>>,
}

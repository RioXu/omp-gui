//! Search state and navigation for the active conversation or document.
use crate::desktop::workspace::*;
mod conversation;
mod documents;

mod state;
pub(in crate::desktop::workspace) use state::SearchState;

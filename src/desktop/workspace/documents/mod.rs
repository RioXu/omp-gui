//! Open document tabs, project refresh and file-tree presentation.
use crate::desktop::workspace::*;
mod file_links;
mod file_sync;
pub(in crate::desktop::workspace) mod file_tree;
mod tabs;
pub(in crate::desktop::workspace) use tabs::{DocumentTab, language_for_path, searchable_diff};

mod state;
pub(in crate::desktop::workspace) use state::{DocumentState, FileEvent};

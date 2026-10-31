//! Composer completion and local configuration-command routing.
use crate::desktop::workspace::*;
mod completion;
mod settings_commands;

mod attachments;
mod state;
pub(in crate::desktop::workspace) use attachments::AttachmentImports;
pub(in crate::desktop::workspace) use state::ComposerState;

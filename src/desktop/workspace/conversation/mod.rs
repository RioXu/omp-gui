//! Session commands, message submission, comments and transcript coordination.
use crate::desktop::workspace::*;
mod comments;
pub(in crate::desktop::workspace) mod demo;
mod queue;
mod session_commands;
pub(in crate::desktop::workspace) mod submission;
mod transcript_layout;

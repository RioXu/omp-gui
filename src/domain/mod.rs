//! Serializable conversation rules and data. No OS, OMP-process or GPUI dependency.
pub(crate) mod activity;
mod comments;
pub(crate) mod file_buffer;
mod identity;
mod messages;
mod native_history;
mod progress;
mod projects;
mod session;
mod snapshots;
mod tool_output;
mod transcript;
mod workspace;
pub(crate) use comments::{Anchor, CodeLines, Comment, Source};
pub(crate) use identity::{hash, id, now};
pub(crate) use messages::{Message, MessageState};
pub use progress::{ProcessBlock, ProcessKind, process_blocks};
pub(crate) use session::Session;
pub(crate) use snapshots::{Snapshot, fenced};
pub(crate) use transcript::{TranscriptRow, transcript_rows};
pub(crate) use workspace::Database;

#[cfg(test)]
mod tests;

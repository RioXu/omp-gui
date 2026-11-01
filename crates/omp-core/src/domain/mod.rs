//! Serializable conversation rules and data. No OS, OMP-process or GPUI dependency.
pub mod activity;
pub mod attachments;
pub use attachments::Attachment;
mod comments;
pub mod file_buffer;
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
pub use comments::{Anchor, CodeLines, Comment, Source};
pub use identity::{hash, id, now};
pub use messages::{Message, MessageState};
pub use progress::{ProcessBlock, ProcessKind, process_blocks};
pub use session::Session;
pub use snapshots::{Snapshot, fenced};
pub use transcript::{TranscriptRow, transcript_rows};
pub use workspace::Database;

#[cfg(test)]
mod tests;

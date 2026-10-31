//! Native OMP client. No desktop/UI dependency is allowed in this adapter.
mod client;
mod diagnostics;
mod framing;
mod messages;
pub mod sessions;
pub use client::{Client, Event};
pub use diagnostics::probe;
pub use messages::message_text;
pub(crate) mod installation;

//! Native OMP client. No desktop/UI dependency is allowed in this adapter.
mod client;
mod diagnostics;
mod display;
pub use display::display_message;
mod framing;
pub use framing::encode as encode_transport_frame;
mod messages;
pub mod sessions;
pub use client::{Client, Event};
pub use diagnostics::probe;
pub use messages::message_text;
pub mod installation;

/// Session-level transport shared by local child processes and remote adapters.
pub trait SessionConnection: Send {
    fn send(&self, frame: serde_json::Value) -> anyhow::Result<()>;
    fn poll(&self, limit: usize) -> Vec<Event>;
    /// Explicitly release this owner's idle writer. Ordinary drop still permits
    /// remote work to survive a lost GUI or SSH transport.
    fn release_idle(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

impl SessionConnection for Client {
    fn send(&self, frame: serde_json::Value) -> anyhow::Result<()> {
        Client::send(self, frame)
    }
    fn poll(&self, limit: usize) -> Vec<Event> {
        self.events.try_iter().take(limit).collect()
    }
}

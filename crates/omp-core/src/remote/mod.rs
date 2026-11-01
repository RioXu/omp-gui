//! Versioned remote host operations carried inside an authenticated SSH connection.
pub mod operations;
pub mod protocol;
pub mod receipts;
mod requests;
pub mod session;
pub mod ssh;
pub use ssh::RemoteHost;

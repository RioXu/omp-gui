//! Native desktop composition and feature-owned workspace presentation.
mod bootstrap;
pub(crate) mod markdown;
pub(crate) mod project_picker;
pub(crate) mod services;
pub(crate) mod settings;
mod theme;
pub(crate) mod workspace;

/// Run the Windows desktop client.
pub fn run() {
    bootstrap::run();
}

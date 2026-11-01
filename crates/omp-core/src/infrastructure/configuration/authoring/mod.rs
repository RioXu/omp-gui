//! Native agent documents, scoped role assignments and model metadata for forms.
mod agents;
mod models;
mod roles;
pub use agents::AgentDraft;
pub use models::{available_models, choices, effort_options, split_effort};
pub use roles::{RoleDraft, role_names};

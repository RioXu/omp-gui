//! Native agent documents, scoped role assignments and model metadata for forms.
mod agents;
mod models;
mod roles;
pub(crate) use agents::AgentDraft;
pub(crate) use models::{available_models, choices, effort_options, split_effort};
pub(crate) use roles::{RoleDraft, role_names};

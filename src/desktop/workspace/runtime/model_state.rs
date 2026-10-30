//! Available models and provider-specific reasoning controls for the current connection.
use serde_json::Value;

pub(in crate::desktop::workspace) struct ModelControlState {
    pub(in crate::desktop::workspace) available: Vec<Value>,
    pub(in crate::desktop::workspace) current: Value,
    pub(in crate::desktop::workspace) levels: Vec<String>,
    pub(in crate::desktop::workspace) levels_request: Option<String>,
    pub(in crate::desktop::workspace) auto: bool,
    pub(in crate::desktop::workspace) entries_cursor: Option<String>,
    pub(in crate::desktop::workspace) pending: Option<(String, Option<String>)>,
    pub(in crate::desktop::workspace) label: String,
    pub(in crate::desktop::workspace) thinking: String,
}

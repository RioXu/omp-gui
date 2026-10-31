use super::{ProcessBlock, id, now};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Message {
    pub id: String,
    pub role: String,
    pub text: String,
    pub time: u64,
    #[serde(default)]
    pub comments: Vec<String>,
    #[serde(default)]
    pub process: Vec<ProcessBlock>,
    #[serde(default)]
    pub state: MessageState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_id: Option<String>,
    /// Wire prompt can include GUI comment context, unlike the displayed text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) enum MessageState {
    #[default]
    Normal,
    Queued,
    Unconfirmed,
    Cancelled,
    Interrupted,
}
impl Message {
    pub fn new(role: &str, text: String) -> Self {
        Self {
            id: id(),
            role: role.into(),
            text,
            time: now(),
            comments: vec![],
            process: vec![],
            state: MessageState::Normal,
            native_id: None,
            native_text: None,
            label: None,
            details: serde_json::Value::Null,
        }
    }
}

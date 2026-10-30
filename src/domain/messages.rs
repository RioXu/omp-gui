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
        }
    }
}

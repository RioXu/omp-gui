//! Durable attachment metadata; payloads live outside workspace.json.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub bytes: u64,
}

pub fn merge_attachments(target: &mut Vec<Attachment>, attachments: Vec<Attachment>) {
    for attachment in attachments {
        if !target.iter().any(|a| a.id == attachment.id) {
            target.push(attachment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Message, MessageState, Session};
    #[test]
    fn drafts_survive_serialization_queue_recovery_and_native_history_merge() {
        let mut session = Session::new("project".into());
        let attachment = Attachment {
            id: "a".into(),
            name: "image.png".into(),
            path: "cache/image.png".into(),
            original_path: None,
            mime: Some("image/png".into()),
            bytes: 100,
        };
        session.draft_attachments.push(attachment.clone());
        assert!(!session.is_untouched());
        let mut message = Message::new("user", "请看图片".into());
        message.native_text = Some("wire prompt".into());
        message.attachments.push(attachment);
        message.state = MessageState::Queued;
        session.messages.push(message);
        let mut restored: Session =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        restored.recover_unconfirmed_queue();
        restored.recover_unconfirmed_queue();
        assert_eq!(restored.draft_attachments.len(), 1);
        assert_eq!(restored.draft, "请看图片");
        restored.merge_native_history(vec![Message::new("user", "wire prompt".into())]);
        assert_eq!(restored.messages[0].text, "请看图片");
        assert_eq!(restored.messages[0].attachments.len(), 1);
    }
}

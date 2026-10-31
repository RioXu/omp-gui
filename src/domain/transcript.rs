use super::Message;
use std::ops::Range;

/// Display grouping never changes original messages or comment anchors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRow {
    User(usize),
    Assistant(Range<usize>),
    Notice(usize),
}
impl TranscriptRow {
    pub fn contains(&self, message: usize) -> bool {
        match self {
            Self::User(ix) | Self::Notice(ix) => *ix == message,
            Self::Assistant(range) => range.contains(&message),
        }
    }
}
pub(crate) fn transcript_rows(messages: &[Message]) -> Vec<TranscriptRow> {
    let mut rows = Vec::new();
    for (ix, message) in messages.iter().enumerate() {
        if message.state == super::MessageState::Queued {
            continue;
        }
        if message.role == "assistant" {
            if let Some(TranscriptRow::Assistant(range)) = rows.last_mut() {
                range.end = ix + 1;
            } else {
                rows.push(TranscriptRow::Assistant(ix..ix + 1));
            }
        } else if message.role == "user" {
            rows.push(TranscriptRow::User(ix));
        } else {
            rows.push(TranscriptRow::Notice(ix));
        }
    }
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_input_does_not_split_the_running_turn_until_delivery() {
        let mut messages = vec![
            Message::new("user", "first".into()),
            Message::new("assistant", "working".into()),
            Message::new("user", "next".into()),
        ];
        messages[2].state = super::super::MessageState::Queued;
        assert_eq!(
            transcript_rows(&messages),
            vec![TranscriptRow::User(0), TranscriptRow::Assistant(1..2)]
        );
        messages[2].state = super::super::MessageState::Normal;
        assert_eq!(transcript_rows(&messages).len(), 3);
    }
    #[test]
    fn tool_rounds_group_without_changing_source_indexes() {
        let messages = [
            "user",
            "assistant",
            "assistant",
            "assistant",
            "user",
            "assistant",
        ]
        .map(|role| Message::new(role, String::new()));
        let rows = transcript_rows(&messages);
        assert_eq!(
            rows,
            vec![
                TranscriptRow::User(0),
                TranscriptRow::Assistant(1..4),
                TranscriptRow::User(4),
                TranscriptRow::Assistant(5..6)
            ]
        );
        assert!(rows[1].contains(2));
    }
    #[test]
    fn inserted_advice_and_peer_messages_are_separate_from_user_and_assistant_turns() {
        let messages = ["user", "assistant", "custom", "custom", "assistant"]
            .map(|role| Message::new(role, "text".into()));
        assert_eq!(
            transcript_rows(&messages),
            vec![
                TranscriptRow::User(0),
                TranscriptRow::Assistant(1..2),
                TranscriptRow::Notice(2),
                TranscriptRow::Notice(3),
                TranscriptRow::Assistant(4..5)
            ]
        );
    }
}

use super::Message;
use std::ops::Range;

/// Display grouping never changes original messages or comment anchors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRow {
    User(usize),
    Assistant(Range<usize>),
}
impl TranscriptRow {
    pub fn contains(&self, message: usize) -> bool {
        match self {
            Self::User(ix) => *ix == message,
            Self::Assistant(range) => range.contains(&message),
        }
    }
}
pub(crate) fn transcript_rows(messages: &[Message]) -> Vec<TranscriptRow> {
    let mut rows = Vec::new();
    for (ix, message) in messages.iter().enumerate() {
        if message.role == "assistant" {
            if let Some(TranscriptRow::Assistant(range)) = rows.last_mut() {
                range.end = ix + 1;
            } else {
                rows.push(TranscriptRow::Assistant(ix..ix + 1));
            }
        } else {
            rows.push(TranscriptRow::User(ix));
        }
    }
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
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
}

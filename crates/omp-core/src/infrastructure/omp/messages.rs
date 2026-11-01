//! Extract public answer text from OMP message payloads.
use serde_json::Value;
pub fn message_text(message: &Value) -> String {
    if let Some(s) = message["content"].as_str() {
        return s.into();
    }
    message["content"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn extracts_only_public_text() {
        assert_eq!(
            message_text(
                &json!({"content":[{"type":"thinking","thinking":"private"},{"type":"text","text":"reply"}]})
            ),
            "reply"
        );
    }
}

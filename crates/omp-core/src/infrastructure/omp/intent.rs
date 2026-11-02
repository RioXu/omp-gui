//! Project OMP's public tool intent onto the same working line as its TUI.
use serde_json::Value;

fn normalize(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim().trim_end_matches('.').trim();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

pub fn working_intent(frame: &Value) -> Option<String> {
    match frame["type"].as_str()? {
        "tool_execution_start" => {
            normalize(&frame["intent"]).or_else(|| normalize(&frame["args"]["i"]))
        }
        "message_start" | "message_update" | "message_end"
            if frame["message"]["role"] == "assistant" =>
        {
            frame["message"]["content"]
                .as_array()?
                .iter()
                .rev()
                .filter(|block| block["type"] == "toolCall")
                .find_map(|block| {
                    normalize(&block["arguments"]["i"]).or_else(|| normalize(&block["intent"]))
                })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_tool_intents_are_single_line_and_latest_usable_call_wins() {
        let frame = json!({"type":"message_update", "message":{"role":"assistant", "content":[
            {"type":"thinking", "thinking":"not an intent"},
            {"type":"toolCall", "arguments":{"i":"Earlier"}},
            {"type":"toolCall", "arguments":{"i":"  正在核对\n项目文件 ...  "}},
            {"type":"toolCall", "arguments":{"i":{"partial":true}}}
        ]}});
        assert_eq!(working_intent(&frame).as_deref(), Some("正在核对 项目文件"));
        assert_eq!(working_intent(&json!({"type":"tool_execution_start", "intent":"Inspect files...", "args":{"i":"fallback"}})).as_deref(), Some("Inspect files"));
        assert_eq!(
            working_intent(&json!({"type":"tool_execution_start", "args":{"i":"fallback"}}))
                .as_deref(),
            Some("fallback")
        );
    }

    #[test]
    fn partial_or_empty_intents_and_other_messages_are_ignored() {
        for value in [
            json!(null),
            json!(false),
            json!(12),
            json!({}),
            json!(" ... "),
        ] {
            assert!(
                working_intent(&json!({"type":"tool_execution_start", "intent":value})).is_none()
            );
        }
        assert!(working_intent(&json!({"type":"message_update", "message":{"role":"user", "content":[{"type":"toolCall", "arguments":{"i":"user text"}}]}})).is_none());
    }
}

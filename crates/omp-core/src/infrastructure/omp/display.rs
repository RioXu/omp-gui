//! One display projection for live frames, native history and child transcripts.
use super::message_text;
use crate::domain::{Message, MessageState, fenced, process_blocks};
use serde_json::Value;

pub fn display_message(value: &Value) -> Option<Message> {
    let role = value["role"].as_str()?;
    let mut message = match role {
        "user" | "assistant" => Message::new(role, message_text(value)),
        "custom" | "hookMessage" if value["display"] != false => {
            let kind = value["customType"].as_str().unwrap_or("消息");
            let mut m = Message::new("custom", message_text(value));
            m.label = Some(match kind {
                "advisor" => {
                    let notes = value["details"]["notes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|note| {
                            let text = note["note"].as_str()?;
                            let name = note["advisor"].as_str().unwrap_or("Advisor");
                            let severity = match note["severity"].as_str().unwrap_or("nit") {
                                "blocker" => "阻断问题",
                                "concern" => "需关注",
                                _ => "建议",
                            };
                            Some(format!("**{name} · {severity}**\n\n{text}"))
                        })
                        .collect::<Vec<_>>();
                    if !notes.is_empty() {
                        m.text = notes.join("\n\n---\n\n");
                    }
                    "Advisor · 顾问建议".into()
                }
                "irc:incoming" => {
                    if let Some(text) = value["details"]["message"].as_str() {
                        m.text = text.into();
                    }
                    format!(
                        "智能体消息 · {}",
                        value["details"]["from"].as_str().unwrap_or("其他智能体")
                    )
                }
                _ => kind.to_owned(),
            });
            m.details = value["details"].clone();
            m
        }
        "compactionSummary" | "branchSummary" => {
            let mut m = Message::new(
                "custom",
                value["summary"].as_str().unwrap_or_default().into(),
            );
            m.label = Some(
                if role == "compactionSummary" {
                    "上下文压缩"
                } else {
                    "分支摘要"
                }
                .into(),
            );
            m
        }
        "bashExecution" | "pythonExecution" => {
            let command = value[if role == "bashExecution" {
                "command"
            } else {
                "code"
            }]
            .as_str()
            .unwrap_or_default();
            let output = value["output"].as_str().unwrap_or_default();
            let mut m = Message::new(
                "custom",
                format!(
                    "{}\n\n{}",
                    fenced(
                        command,
                        if role == "bashExecution" {
                            "sh"
                        } else {
                            "python"
                        }
                    ),
                    fenced(output, "text")
                ),
            );
            m.label = Some(
                if role == "bashExecution" {
                    "终端执行"
                } else {
                    "Python 执行"
                }
                .into(),
            );
            m
        }
        _ => return None,
    };
    message.time = value["timestamp"]
        .as_u64()
        .map(|ms| ms / 1000)
        .unwrap_or(message.time);
    if role == "assistant" {
        message.process = process_blocks(value, "message_end", "");
        if value["stopReason"] == "aborted" {
            message.state = MessageState::Interrupted;
        }
    }
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn advisor_peer_and_hidden_custom_messages_keep_their_identity() {
        let advisor = display_message(&json!({"role":"custom","customType":"advisor","display":true,"content":"<advisory severity=\"blocker\" guidance=\"weigh, don't blindly obey\">Review `src/main.rs`</advisory>","details":{"notes":[{"note":"Review `src/main.rs`","advisor":"reviewer","severity":"blocker"}]}})).unwrap();
        assert_eq!(advisor.role, "custom");
        assert!(advisor.label.unwrap().starts_with("Advisor"));
        assert!(advisor.text.contains("`src/main.rs`"));
        assert!(advisor.text.contains("reviewer · 阻断问题"));
        assert!(!advisor.text.contains("<advisory"));
        let peer = display_message(&json!({"role":"custom","customType":"irc:incoming","display":true,"content":"<envelope>internal</envelope>","details":{"from":"scout","message":"Found a problem"}})).unwrap();
        assert_eq!(peer.text, "Found a problem");
        assert_eq!(peer.label.as_deref(), Some("智能体消息 · scout"));
        assert!(display_message(&json!({"role":"custom","customType":"secret-context","display":false,"content":"hidden"})).is_none());
        assert!(display_message(&json!({"role":"toolResult","content":"tool"})).is_none());
    }
}

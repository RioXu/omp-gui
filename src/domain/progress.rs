//! Public process blocks received from OMP, separate from the answer text.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProcessKind {
    Thinking,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessBlock {
    pub id: String,
    pub kind: ProcessKind,
    pub label: String,
    pub text: String,
    #[serde(skip)]
    pub streaming: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub patch: Option<String>,
}

pub fn process_blocks(message: &Value, event: &str, delta: &str) -> Vec<ProcessBlock> {
    let Some(content) = message["content"].as_array() else {
        return vec![];
    };
    content
        .iter()
        .enumerate()
        .filter_map(|(ix, block)| match block["type"].as_str()? {
            "thinking" => Some(ProcessBlock {
                id: format!("thinking-{ix}"),
                kind: ProcessKind::Thinking,
                label: "思考".into(),
                text: block["thinking"].as_str().unwrap_or_default().into(),
                streaming: event != "message_end"
                    && delta != "thinking_end"
                    && ix + 1 == content.len(),
                result: None,
                patch: None,
            }),
            "toolCall" => Some(ProcessBlock {
                id: block["id"].as_str().unwrap_or_default().into(),
                kind: ProcessKind::Tool,
                label: block["name"].as_str().unwrap_or("工具调用").into(),
                text: serde_json::to_string_pretty(&block["arguments"]).unwrap_or_default(),
                streaming: false,
                result: None,
                patch: None,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn thinking_closes_at_block_boundary_and_tools_never_auto_expand() {
        let mut m = json!({"content":[{"type":"thinking","thinking":"visible reasoning"}]});
        assert!(process_blocks(&m, "message_update", "thinking_delta")[0].streaming);
        assert!(!process_blocks(&m, "message_update", "thinking_end")[0].streaming);
        m["content"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"toolCall","id":"t1","name":"read","arguments":{"path":"a"}}));
        let blocks = process_blocks(&m, "message_update", "toolcall_delta");
        assert!(blocks.iter().all(|b| !b.streaming));
        assert_eq!(blocks[1].id, "t1");
        assert!(
            !serde_json::to_string(&blocks)
                .unwrap()
                .contains("streaming")
        );
    }
}

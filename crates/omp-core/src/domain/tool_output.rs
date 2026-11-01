use super::{ProcessBlock, Snapshot};
use serde_json::Value;
impl super::Session {
    pub fn start_tool(&mut self, call_id: &str, name: &str, args: &Value, run_start: usize) {
        if self
            .messages
            .iter()
            .any(|m| m.process.iter().any(|b| b.id == call_id))
        {
            return;
        }
        let mut blocks = super::process_blocks(
            &serde_json::json!({"content":[{"type":"toolCall","id":call_id,"name":name,"arguments":args}]}),
            "message_end",
            "",
        );
        if let Some(message) = self
            .messages
            .iter_mut()
            .skip(run_start)
            .rev()
            .find(|m| m.role == "assistant")
        {
            message.process.append(&mut blocks);
        } else {
            let mut message = super::Message::new("assistant", String::new());
            message.process = blocks;
            self.messages.push(message);
        }
    }
}

impl ProcessBlock {
    pub fn is_read(&self) -> bool {
        matches!(self.label.as_str(), "read" | "read_file")
    }
    pub fn is_edit(&self) -> bool {
        matches!(
            self.label.as_str(),
            "edit" | "write" | "apply_patch" | "ast_edit" | "write_file"
        )
    }
    pub fn path(&self) -> String {
        let args: Value = serde_json::from_str(&self.text).unwrap_or_default();
        args["path"]
            .as_str()
            .or_else(|| args["file_path"].as_str())
            .unwrap_or_default()
            .to_owned()
    }
    pub fn command(&self) -> String {
        let args: Value = serde_json::from_str(&self.text).unwrap_or_default();
        args["command"]
            .as_str()
            .or_else(|| args["code"].as_str())
            .or_else(|| args["script"].as_str())
            .unwrap_or_default()
            .to_owned()
    }
    pub fn set_result(&mut self, text: String, details: &Value) {
        self.result = Some(text);
        if self.is_edit() {
            self.patch = edit_patch(details, &self.path());
        }
    }
}
/// Prefer OMP's source-of-truth snapshots. Never reconstruct files from truncated previews.
pub fn edit_patch(details: &Value, fallback_path: &str) -> Option<String> {
    if let Some(files) = details["perFileResults"].as_array() {
        let patches = files
            .iter()
            .filter_map(|d| edit_patch(d, fallback_path))
            .collect::<Vec<_>>();
        return (!patches.is_empty()).then(|| patches.join("\n"));
    }
    let path = details["path"].as_str().unwrap_or(fallback_path);
    if details["snapshotsPruned"] != true
        && (details["oldText"].is_string() || details["newText"].is_string())
    {
        let before = details["oldText"].as_str().unwrap_or_default();
        let after = details["newText"].as_str().unwrap_or_default();
        return Some(Snapshot::new(path.into(), before.into(), after.into()).patch);
    }
    details["diff"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn read_start_creates_visible_record_even_without_assistant_tool_snapshot() {
        let mut s = super::super::Session::new("project".into());
        s.start_tool(
            "read-1",
            "read",
            &json!({"path":"https://example.test/document"}),
            0,
        );
        s.start_tool(
            "read-1",
            "read",
            &json!({"path":"https://example.test/document"}),
            0,
        );
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].process.len(), 1);
        assert!(s.messages[0].process[0].is_read());
        assert_eq!(
            s.messages[0].process[0].path(),
            "https://example.test/document"
        );
    }
    #[test]
    fn edit_result_uses_historical_snapshots_and_keeps_multiple_files() {
        let patch=edit_patch(&json!({"perFileResults":[{"path":"a.rs","oldText":"old\n","newText":"new\n"},{"path":"b.rs","oldText":"a\n","newText":"b\n"}]}),"").unwrap();
        assert!(patch.contains("-old"));
        assert!(patch.contains("+new"));
        assert!(patch.contains("b.rs"));
    }
    #[test]
    fn command_display_omits_transport_parameters() {
        let b=super::super::process_blocks(&json!({"content":[{"type":"toolCall","id":"x","name":"bash","arguments":{"command":"git status","timeout":1000}}]}),"message_end","").remove(0);
        assert_eq!(b.command(), "git status");
    }
}

use super::{ProcessBlock, Snapshot};
use serde_json::Value;

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
pub(crate) fn edit_patch(details: &Value, fallback_path: &str) -> Option<String> {
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

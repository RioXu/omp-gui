use super::{id, now};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub path: PathBuf,
    pub text: String,
    pub before: String,
    pub patch: String,
    pub created: u64,
}
impl Snapshot {
    pub fn new(path: PathBuf, before: String, text: String) -> Self {
        // Normalize Windows line endings for presentation only; immutable
        // before/text snapshots retain the actual bytes returned from disk.
        let display_before = before.replace("\r\n", "\n");
        let display_text = text.replace("\r\n", "\n");
        let patch = similar::TextDiff::from_lines(&display_before, &display_text)
            .unified_diff()
            .context_radius(4)
            .header(
                &format!("a/{}", path.display()),
                &format!("b/{}", path.display()),
            )
            .to_string();
        Self {
            id: id(),
            path,
            text,
            before,
            patch,
            created: now(),
        }
    }
    pub fn document(&self) -> String {
        fenced(
            if self.patch.is_empty() {
                &self.text
            } else {
                &self.patch
            },
            if self.patch.is_empty() { "" } else { "diff" },
        )
    }
}
pub fn fenced(text: &str, lang: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    format!("{fence}{lang}\n{text}\n{fence}")
}

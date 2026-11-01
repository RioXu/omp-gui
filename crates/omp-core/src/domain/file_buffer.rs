use std::sync::Arc;

/// The saved revision is independent of immutable comment snapshots.
#[derive(Clone)]
pub struct FileBuffer {
    disk: Arc<str>,
    editor: Arc<str>,
}

impl FileBuffer {
    pub fn new(disk: &str) -> Self {
        Self {
            disk: disk.into(),
            editor: Self::decode(disk).into(),
        }
    }
    pub fn decode(disk: &str) -> String {
        disk.strip_prefix('\u{feff}')
            .unwrap_or(disk)
            .replace("\r\n", "\n")
    }
    pub fn disk_text(&self) -> &str {
        &self.disk
    }
    pub fn editor_text(&self) -> &str {
        &self.editor
    }
    pub fn is_dirty(&self, text: &str) -> bool {
        text != self.editor.as_ref()
    }
    pub fn encode(&self, text: &str) -> String {
        let crlf = self
            .disk
            .find('\n')
            .is_some_and(|i| i > 0 && self.disk.as_bytes()[i - 1] == b'\r');
        let text = text.replace("\r\n", "\n");
        let text = if crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        };
        if self.disk.starts_with('\u{feff}') {
            format!("\u{feff}{text}")
        } else {
            text
        }
    }
    pub fn saved(&mut self, disk: String, editor: String) {
        self.disk = disk.into();
        self.editor = editor.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_undo_and_save_use_the_saved_revision() {
        let mut buffer = FileBuffer::new("before\n");
        assert!(!buffer.is_dirty("before\n"));
        assert!(buffer.is_dirty("after\n"));
        assert!(!buffer.is_dirty("before\n"));
        buffer.saved("after\n".into(), "after\n".into());
        assert!(!buffer.is_dirty("after\n"));
        assert!(
            buffer.is_dirty("before\n"),
            "undo after saving is a new unsaved edit"
        );
    }
    #[test]
    fn utf8_bom_and_line_endings_survive_editing() {
        for original in [
            "中文\nnext\n",
            "\u{feff}中文\r\nnext\r\n",
            "中文\r\nnext\r\n",
        ] {
            let buffer = FileBuffer::new(original);
            assert_eq!(buffer.editor_text(), "中文\nnext\n");
            assert_eq!(buffer.encode(buffer.editor_text()), original);
            let edited = buffer.encode("edited\nnext\n");
            assert_eq!(FileBuffer::decode(&edited), "edited\nnext\n");
            assert_eq!(
                edited.starts_with('\u{feff}'),
                original.starts_with('\u{feff}')
            );
        }
    }
}

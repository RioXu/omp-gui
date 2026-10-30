use super::{hash, id, now};
use serde::{Deserialize, Serialize};
use std::{ops::Range, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) enum Source {
    Message(String),
    File { path: PathBuf, snapshot: String },
}

/// Immutable source bytes plus a UTF-8 source range, never a mutable line number.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Anchor {
    pub source: Source,
    pub document: String,
    pub digest: String,
    pub range: Range<usize>,
    pub quote: String,
    #[serde(default = "exact_range_default")]
    pub exact_range: bool,
    #[serde(default)]
    pub code_lines: Option<CodeLines>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CodeLines {
    pub original: bool,
    pub end_original: bool,
    pub first: usize,
    pub last: usize,
}
fn exact_range_default() -> bool {
    true
}
impl Anchor {
    pub fn new(
        source: Source,
        document: String,
        range: Range<usize>,
        quote: String,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(!quote.trim().is_empty(), "请选择需要评论的文字");
        anyhow::ensure!(
            range.start < range.end && document.get(range.clone()).is_some(),
            "所选内容已发生变化，请重新选择"
        );
        Ok(Self {
            source,
            digest: hash(&document),
            document,
            range,
            quote,
            exact_range: true,
            code_lines: None,
        })
    }
    pub fn from_quote(source: Source, document: String, quote: String) -> anyhow::Result<Self> {
        let positions: Vec<_> = document.match_indices(&quote).map(|(p, _)| p).collect();
        let exact = positions.len() == 1;
        let range = if exact {
            positions[0]..positions[0] + quote.len()
        } else {
            0..document.len()
        };
        let mut anchor = Self::new(source, document, range, quote)?;
        anchor.exact_range = exact;
        Ok(anchor)
    }
    pub fn label(&self) -> String {
        match &self.source {
            Source::Message(_) => "回复片段".into(),
            Source::File { path, .. } => path.to_string_lossy().into_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Comment {
    pub id: String,
    pub anchor: Anchor,
    pub text: String,
    pub created: u64,
    pub sent: bool,
    pub resolved: bool,
    #[serde(default)]
    pub reply_ids: Vec<String>,
}
impl Comment {
    pub fn new(anchor: Anchor, text: String) -> Self {
        Self {
            id: id(),
            anchor,
            text,
            created: now(),
            sent: false,
            resolved: false,
            reply_ids: vec![],
        }
    }
}

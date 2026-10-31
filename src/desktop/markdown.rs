//! Adapt compact display-math fences to the native Markdown parser without
//! changing persisted messages or their comment/source offsets.
use std::{borrow::Cow, ops::Range};

pub(crate) fn view(
    id: impl Into<gpui_kit::ElementId>,
    source: &str,
) -> gpui_kit::component::text::TextView {
    gpui_kit::component::text::TextView::markdown(id, prepare(source).0.into_owned())
}

#[derive(Clone, Default)]
pub(crate) struct SourceOffsets {
    /// Original byte offsets before which a display-only newline was inserted.
    insertions: Vec<usize>,
}
impl SourceOffsets {
    /// A TextView can still hold the previous asynchronous parse while a new
    /// stream chunk is pending. Never apply the new map to that old document.
    pub(crate) fn matches(&self, display: &str, original: &str) -> bool {
        if display.len() != original.len() + self.insertions.len() {
            return false;
        }
        let (mut source_start, mut display_start) = (0, 0);
        for (ix, &source_end) in self.insertions.iter().enumerate() {
            let display_end = source_end + ix;
            if display.get(display_start..display_end) != original.get(source_start..source_end)
                || display.as_bytes().get(display_end) != Some(&b'\n')
            {
                return false;
            }
            source_start = source_end;
            display_start = display_end + 1;
        }
        display.get(display_start..) == original.get(source_start..)
    }
    pub(crate) fn original_range(&self, display: Range<usize>) -> Range<usize> {
        let original = |at: usize| {
            at - self
                .insertions
                .iter()
                .enumerate()
                .take_while(|(ix, offset)| **offset + ix < at)
                .count()
        };
        original(display.start)..original(display.end)
    }
    pub(crate) fn display_range(&self, original: Range<usize>) -> Range<usize> {
        (original.start + self.insertions.partition_point(|at| *at <= original.start))
            ..(original.end + self.insertions.partition_point(|at| *at < original.end))
    }
}

pub(crate) fn prepare(source: &str) -> (Cow<'_, str>, SourceOffsets) {
    if !source.contains("$$") {
        return (Cow::Borrowed(source), SourceOffsets::default());
    }
    // Use the existing Markdown AST to protect real fenced/indented code and
    // inline code. Formulas inside examples must remain byte-for-byte intact.
    let Ok(ast) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return (Cow::Borrowed(source), SourceOffsets::default());
    };
    fn code_ranges(node: &markdown::mdast::Node, ranges: &mut Vec<Range<usize>>) {
        if matches!(
            node,
            markdown::mdast::Node::Code(_) | markdown::mdast::Node::InlineCode(_)
        ) {
            if let Some(position) = node.position() {
                ranges.push(position.start.offset..position.end.offset);
            }
        } else if let Some(children) = node.children() {
            for child in children {
                code_ranges(child, ranges);
            }
        }
    }
    let mut code = Vec::new();
    code_ranges(&ast, &mut code);
    let protected = |at: usize| code.iter().find(|r| r.contains(&at));
    let bytes = source.as_bytes();
    let delimiter = |at: usize| {
        bytes.get(at..at + 2) == Some(b"$$")
            && bytes.get(at.wrapping_sub(1)) != Some(&b'$')
            && bytes.get(at + 2) != Some(&b'$')
            && bytes[..at]
                .iter()
                .rev()
                .take_while(|b| **b == b'\\')
                .count()
                % 2
                == 0
    };
    let line_start = |at: usize| source[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| source[at..].find('\n').map_or(source.len(), |i| at + i);
    let mut insertions = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find("$$") {
        let open = cursor + offset;
        cursor = open + 2;
        if let Some(range) = protected(open) {
            cursor = range.end;
            continue;
        }
        let prefix = &source[line_start(open)..open];
        if !delimiter(open) || prefix.len() > 3 || !prefix.bytes().all(|b| b == b' ') {
            continue;
        }
        let mut close = None;
        let mut scan = cursor;
        while let Some(offset) = source[scan..].find("$$") {
            let at = scan + offset;
            scan = at + 2;
            if let Some(range) = protected(at) {
                scan = range.end;
                continue;
            }
            if delimiter(at) {
                close = Some(at);
                break;
            }
        }
        let Some(close) = close else { continue };
        cursor = close + 2;
        if !source[open..close].contains('\n') {
            continue;
        }
        if !source[open + 2..line_end(open)].trim().is_empty() {
            insertions.push(open + 2);
        }
        if !source[line_start(close)..close].trim().is_empty() {
            insertions.push(close);
        }
        if !source[close + 2..line_end(close + 2)].trim().is_empty() {
            insertions.push(close + 2);
        }
    }
    if insertions.is_empty() {
        return (Cow::Borrowed(source), SourceOffsets::default());
    }
    let mut display = String::with_capacity(source.len() + insertions.len());
    let mut start = 0;
    for &at in &insertions {
        display.push_str(&source[start..at]);
        display.push('\n');
        start = at;
    }
    display.push_str(&source[start..]);
    (Cow::Owned(display), SourceOffsets { insertions })
}

#[cfg(test)]
mod tests;

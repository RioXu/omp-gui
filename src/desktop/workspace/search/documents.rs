use super::*;
use gpui_kit::base::input::SearchMatcher;
use gpui_kit::component::{diff::DiffLinePosition, highlighter::SyntaxHighlighter};
use std::ops::Range;

impl Workspace {
    pub(in crate::desktop::workspace) fn clear_search_marks(&mut self, cx: &mut Context<Self>) {
        for entry in self.texts.values() {
            entry.state.update(cx, |s, cx| {
                let _ = s.set_range_highlights([], cx);
            });
        }
        if let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.search_state.scope.as_ref())
        {
            if let Some(editor) = &tab.editor {
                editor.update(cx, |e, cx| e.close_search(cx));
            }
            if let Some(diff) = &tab.diff {
                diff.update(cx, |d, cx| d.clear_syntax_highlighter(cx));
            }
            if let Some(entry) = &tab.entry {
                entry.state.update(cx, |s, cx| {
                    let _ = s.set_range_highlights([], cx);
                });
            }
        }
        self.search_state.diff_positions.clear();
        self.search_state.source.clear();
    }

    pub(in crate::desktop::workspace) fn update_document_search(
        &mut self,
        reveal: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref())
            .cloned()
        else {
            return;
        };
        let query = self.search_state.input.read(cx).value().to_string();
        let previous = self.search_state.hits.get(self.search_state.index).cloned();
        self.search_state.hits.clear();
        self.search_state.diff_positions.clear();
        self.search_state.rendered_text.clear();
        self.search_state.source = tab
            .entry
            .as_ref()
            .map(|e| e.body.clone())
            .unwrap_or_default();
        if let Some(diff) = &tab.diff {
            let path = diff
                .read(cx)
                .files()
                .first()
                .map(|f| f.path().clone())
                .unwrap_or_default();
            let hits = diff_matches(&self.search_state.source, &path, &query);
            for (range, position) in hits {
                self.search_state.hits.push((tab.key.clone(), range));
                self.search_state.diff_positions.push(position);
            }
            if !reveal
                && let Some(previous) = previous
                && let Some(ix) = self.search_state.hits.iter().position(|h| h == &previous)
            {
                self.search_state.index = ix;
            }
            self.search_state.index = self
                .search_state
                .index
                .min(self.search_state.hits.len().saturating_sub(1));
            if query.is_empty() {
                diff.update(cx, |d, cx| d.clear_syntax_highlighter(cx));
            } else {
                let theme = cx.theme().highlight_theme.clone();
                let background = cx.theme().warning.opacity(0.32);
                diff.update(cx, |d, cx| {
                    d.set_syntax_highlighter(
                        move |language, text| {
                            let rope = ropey::Rope::from_str(text);
                            let mut highlighter = SyntaxHighlighter::new(language);
                            highlighter.update(None, &rope, None);
                            search_styles(
                                text,
                                &query,
                                highlighter.styles(&(0..text.len()), theme.as_ref()),
                                background,
                            )
                        },
                        cx,
                    )
                });
            }
            if reveal {
                self.reveal_document_search(cx);
            }
        } else if let Some(editor) = &tab.editor {
            editor.update(cx, |e, cx| {
                e.set_search_query(query, true, cx);
                self.search_state.hits = e
                    .search_session()
                    .matcher
                    .matched_ranges()
                    .iter()
                    .cloned()
                    .map(|r| (tab.key.clone(), r))
                    .collect();
                self.search_state.index = e.search_session().matcher.current_match_index();
                if reveal && !self.search_state.hits.is_empty() {
                    // Reveal the current native match without selecting source text.
                    e.next_search_match(cx);
                    e.previous_search_match(cx);
                }
            });
        } else if let Some(entry) = &tab.entry {
            // Historical text that is not a parsed diff keeps its immutable view.
            let rendered = entry.state.read(cx).rendered_text();
            let mut matcher = SearchMatcher::new();
            matcher.update_query(&query, true);
            matcher.update(&ropey::Rope::from_str(rendered.as_str()));
            self.search_state.hits = matcher
                .matched_ranges()
                .iter()
                .cloned()
                .map(|r| (tab.key.clone(), r))
                .collect();
            self.search_state.index = self
                .search_state
                .index
                .min(self.search_state.hits.len().saturating_sub(1));
            let color = cx.theme().warning.opacity(0.32);
            entry.state.update(cx, |s, cx| {
                let _ = s.set_range_highlights(
                    self.search_state
                        .hits
                        .iter()
                        .map(|(_, r)| RangeHighlight::new(r.clone(), color)),
                    cx,
                );
            });
            if reveal {
                self.reveal_document_search(cx);
            }
        }
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn move_document_search(
        &mut self,
        backwards: bool,
        cx: &mut Context<Self>,
    ) {
        let count = self.search_state.hits.len();
        if count == 0 {
            return;
        }
        let editor = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref() && t.diff.is_none())
            .and_then(|t| t.editor.clone());
        if let Some(editor) = editor {
            editor.update(cx, |e, cx| {
                if backwards {
                    e.previous_search_match(cx);
                } else {
                    e.next_search_match(cx);
                }
                self.search_state.index = e.search_session().matcher.current_match_index();
            });
        } else {
            self.search_state.index =
                (self.search_state.index + if backwards { count - 1 } else { 1 }) % count;
            self.reveal_document_search(cx);
        }
        cx.notify();
    }

    fn reveal_document_search(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref())
        else {
            return;
        };
        if let Some(diff) = &tab.diff {
            if let Some(position) = self
                .search_state
                .diff_positions
                .get(self.search_state.index)
            {
                diff.update(cx, |d, cx| d.scroll_to_line(position.clone(), cx));
            }
        } else if let Some(entry) = &tab.entry
            && let Some((_, range)) = self.search_state.hits.get(self.search_state.index)
        {
            entry.state.update(cx, |s, cx| {
                let _ = s.reveal_range(range.clone(), cx);
            });
        }
    }
}

/// Search only code supplied by the patch, counting context once and mapping
/// removed/added lines to the correct side's original source line number.
fn diff_matches(patch: &str, path: &str, query: &str) -> Vec<(Range<usize>, DiffLinePosition)> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut matcher = SearchMatcher::new();
    matcher.update_query(query, true);
    let (mut old, mut new) = (0, 0);
    let mut in_hunk = false;
    let mut offset = 0;
    let mut hits = Vec::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            let mut parts = line.split_whitespace().skip(1);
            let number = |s: Option<&str>| {
                s.and_then(|s| s.get(1..)?.split(',').next()?.parse::<usize>().ok())
                    .unwrap_or(0)
            };
            old = number(parts.next());
            new = number(parts.next());
            in_hunk = true;
        } else if in_hunk {
            let position = match line.as_bytes().first() {
                Some(b'-') => {
                    let p = DiffLinePosition::new(path.to_owned(), DiffSide::Original, old);
                    old += 1;
                    Some(p)
                }
                Some(b'+') => {
                    let p = DiffLinePosition::new(path.to_owned(), DiffSide::Modified, new);
                    new += 1;
                    Some(p)
                }
                Some(b' ') => {
                    let p = DiffLinePosition::new(path.to_owned(), DiffSide::Modified, new);
                    old += 1;
                    new += 1;
                    Some(p)
                }
                Some(b'\\') => None,
                _ => {
                    in_hunk = false;
                    None
                }
            };
            if let Some(position) = position {
                matcher.update(&ropey::Rope::from_str(
                    line[1..].trim_end_matches(['\r', '\n']),
                ));
                hits.extend(matcher.matched_ranges().iter().map(|r| {
                    (
                        (offset + 1 + r.start)..(offset + 1 + r.end),
                        position.clone(),
                    )
                }));
            }
        }
        offset += line.len();
    }
    hits
}

/// Retain native syntax styles while adding search backgrounds. Both inputs
/// contain non-overlapping UTF-8 ranges; split at their combined boundaries.
fn search_styles(
    text: &str,
    query: &str,
    styles: Vec<(Range<usize>, HighlightStyle)>,
    background: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut matcher = SearchMatcher::new();
    matcher.update_query(query, true);
    matcher.update(&ropey::Rope::from_str(text));
    let matches = matcher.matched_ranges();
    let mut boundaries = vec![0, text.len()];
    for range in styles.iter().map(|(r, _)| r).chain(matches.iter()) {
        boundaries.extend([range.start, range.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let (mut syntax, mut found) = (0, 0);
    let mut result = Vec::new();
    for pair in boundaries.windows(2) {
        let range = pair[0]..pair[1];
        while syntax < styles.len() && styles[syntax].0.end <= range.start {
            syntax += 1;
        }
        while found < matches.len() && matches[found].end <= range.start {
            found += 1;
        }
        let mut style = if syntax < styles.len() && styles[syntax].0.start <= range.start {
            styles[syntax].1
        } else {
            HighlightStyle::default()
        };
        if found < matches.len() && matches[found].start <= range.start {
            style.background_color = Some(background);
        }
        result.push((range, style));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{diff_matches, search_styles};
    use gpui_kit::{HighlightStyle, component::diff::DiffSide, rgb};

    #[gpui_kit::test]
    fn plain_text_diff_paints_search_backgrounds(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::component::diff::{Diff, DiffFile, DiffState};
        use gpui_kit::{
            AppContext, Context, Entity, IntoElement, Render, VisualTestContext, Window,
            WindowOptions, div, prelude::*,
        };
        struct Fixture {
            diff: Entity<DiffState>,
        }
        impl Render for Fixture {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().child(Diff::new(&self.diff).size_full())
            }
        }
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| Fixture {
                    diff: cx.new(|cx| {
                        let files = DiffFile::parse(
                            "--- a/note.txt\n+++ b/note.txt\n@@ -0,0 +1 @@\n+中文 needle\n",
                        )
                        .unwrap();
                        DiffState::new(
                            crate::desktop::workspace::documents::searchable_diff(files),
                            cx,
                        )
                        .with_syntax_highlighter(|_, text| {
                            search_styles(text, "needle", Vec::new(), rgb(0x12abcd).into())
                        })
                    }),
                })
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window, cx);
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        cx.run_until_parked();
        assert!(cx.update(|w, _| {
            w.painted_quads()
                .iter()
                .any(|q| q.background == gpui_kit::Background::from(rgb(0x12abcd)))
        }));
    }
    #[test]
    fn diff_search_counts_context_once_and_keeps_original_line_numbers() {
        let patch = "--- a/needle.rs\n+++ b/needle.rs\n@@ -40,3 +80,3 @@\n needle 上下文\n-needle 旧\n+needle 新 needle\n 尾部\n@@ -100 +200 @@\n-needle 删除\n+完成\n";
        let hits = diff_matches(patch, "needle.rs", "needle");
        assert_eq!(hits.len(), 5);
        assert_eq!(
            hits.iter()
                .map(|(_, p)| (p.side(), p.line()))
                .collect::<Vec<_>>(),
            vec![
                (DiffSide::Modified, 80),
                (DiffSide::Original, 41),
                (DiffSide::Modified, 81),
                (DiffSide::Modified, 81),
                (DiffSide::Original, 100)
            ]
        );
        for (range, _) in hits {
            assert_eq!(&patch[range], "needle");
        }
        assert!(diff_matches(patch, "needle.rs", "").is_empty());
    }
    #[test]
    fn search_background_preserves_syntax_and_unicode_boundaries() {
        let text = "let 中文 = 中文;";
        let foreground = rgb(0x123456).into();
        let background = rgb(0xe6d89c).into();
        let styles = search_styles(
            text,
            "中文",
            vec![(
                0..text.len(),
                HighlightStyle {
                    color: Some(foreground),
                    ..Default::default()
                },
            )],
            background,
        );
        let marked = styles
            .iter()
            .filter(|(_, s)| s.background_color == Some(background))
            .collect::<Vec<_>>();
        assert_eq!(marked.len(), 2);
        for (range, style) in marked {
            assert_eq!(&text[range.clone()], "中文");
            assert_eq!(style.color, Some(foreground));
        }
        assert!(styles.windows(2).all(|p| p[0].0.end == p[1].0.start));
    }
}

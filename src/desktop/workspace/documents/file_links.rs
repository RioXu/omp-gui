//! Keep Markdown source/selection intact through gpui-kit's inline plugin.
use crate::desktop::workspace::*;
use crate::infrastructure::filesystem::file_references;
use gpui_kit::component::{
    ActiveTheme,
    link::Link,
    text::{
        MarkdownExtensions, MarkdownNode, MarkdownParseContext, MarkdownPlugin, RenderedText,
        TextView, markdown_ast,
    },
};

struct FileLinks {
    root: PathBuf,
    remote: bool,
    references: HashMap<String, PathBuf>,
    session: String,
    workspace: WeakEntity<Workspace>,
    query: String,
    active: Option<(RenderedText, std::ops::Range<usize>)>,
}
fn local_match(
    container: std::ops::Range<usize>,
    hit: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    if hit.start < container.start || hit.end > container.end {
        return None;
    }
    Some(hit.start - container.start..hit.end - container.start)
}
fn link_matches(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    if query.is_empty() {
        return vec![];
    }
    let mut matcher = gpui_kit::base::input::SearchMatcher::new();
    matcher.update_query(query, true);
    matcher.update(&ropey::Rope::from_str(text));
    matcher.matched_ranges().as_ref().clone()
}
impl MarkdownPlugin for FileLinks {
    fn name(&self) -> &str {
        "project-file-reference"
    }
    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::InlineCode(code) = node else {
            return None;
        };
        let path = if self.remote {
            self.references.get(code.value.trim()).cloned()
        } else {
            file_references::resolve(&self.root, &code.value)
        }?;
        Some(
            MarkdownNode::new(self.name(), path)
                .text(code.value.clone())
                .markdown(cx.node_source(node).unwrap_or_default().to_owned()),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let path = node.data::<PathBuf>().cloned();
        let workspace = self.workspace.clone();
        let session = self.session.clone();
        let active = self.active.as_ref().and_then(|(rendered, hit)| {
            let range = rendered.range_for_source(node.source_range()?)?;
            local_match(range, hit)
        });
        let highlights = link_matches(node.as_text(), &self.query)
            .into_iter()
            .map(|range| {
                let color = if active.as_ref() == Some(&range) {
                    cx.theme().selection
                } else {
                    cx.theme().warning.opacity(0.28)
                };
                (
                    range,
                    HighlightStyle {
                        background_color: Some(color),
                        ..Default::default()
                    },
                )
            })
            .collect::<Vec<_>>();
        Link::new(SharedString::from(format!(
            "file-ref-{}",
            node.source_range().map(|r| r.start).unwrap_or(0)
        )))
        .font_family("Cascadia Code")
        .bg(cx.theme().muted)
        .rounded_sm()
        .px_1()
        .child(StyledText::new(node.as_text().to_owned()).with_highlights(highlights))
        .on_click(move |_, w, cx| {
            if let Some(path) = &path {
                let _ = workspace.update(cx, |this, cx| {
                    if this.session().is_none_or(|owner| owner.id != session) {
                        return;
                    }
                    this.open_document(path.clone(), false);
                    this.focus_document_editor(w, cx);
                    cx.notify();
                });
            }
        })
    }
}

impl Workspace {
    pub(in crate::desktop::workspace) fn file_link_view(
        &self,
        view: TextView,
        cx: &Context<Self>,
    ) -> TextView {
        self.link_view(view, None, cx)
    }
    pub(in crate::desktop::workspace) fn message_file_link_view(
        &self,
        view: TextView,
        message: &str,
        cx: &Context<Self>,
    ) -> TextView {
        self.link_view(view, Some(message), cx)
    }
    fn link_view(&self, view: TextView, message: Option<&str>, cx: &Context<Self>) -> TextView {
        let Some(session) = self.session() else {
            return view;
        };
        view.markdown_extensions(
            MarkdownExtensions::default()
                .parser_revision(self.document_state.file_link_revision)
                .plugin(FileLinks {
                    root: session.cwd().to_owned(),
                    remote: session.environment_id() != "local",
                    references: self.document_state.references.resolved.clone(),
                    session: session.id.clone(),
                    workspace: cx.entity().downgrade(),
                    query: if message.is_some()
                        && self.search_state.open
                        && self.document_state.active.is_none()
                    {
                        self.search_state.input.read(cx).value().to_string()
                    } else {
                        String::new()
                    },
                    active: self
                        .search_state
                        .hits
                        .get(self.search_state.index)
                        .filter(|(id, _)| Some(id.as_str()) == message)
                        .and_then(|(id, range)| {
                            self.runtime
                                .texts
                                .get(id)
                                .map(|entry| (entry.state.read(cx).rendered_text(), range.clone()))
                        }),
                }),
        )
    }
}

pub(in crate::desktop::workspace) struct ReferenceState {
    pub(super) resolved: HashMap<String, PathBuf>,
    pub(super) request: String,
    loading: bool,
    checked: Instant,
}
impl Default for ReferenceState {
    fn default() -> Self {
        Self {
            resolved: Default::default(),
            request: String::new(),
            loading: false,
            checked: Instant::now() - Duration::from_secs(3),
        }
    }
}

impl Workspace {
    pub(in crate::desktop::workspace) fn refresh_file_references(&mut self) {
        let Some(session) = self
            .session()
            .filter(|session| session.environment_id() != "local")
        else {
            return;
        };
        if self.document_state.references.loading
            || self.document_state.references.checked.elapsed() < Duration::from_secs(2)
        {
            return;
        }
        let bodies = self
            .runtime
            .texts
            .values()
            .map(|entry| entry.body.clone())
            .collect::<Vec<_>>();
        let request = format!(
            "{}:{}:{}",
            session.id,
            self.document_state.file_link_revision,
            crate::domain::hash(&bodies.join("\n"))
        );
        let session_id = session.id.clone();
        let root = session.host_cwd();
        self.document_state.references.checked = Instant::now();
        if self.document_state.references.request == request {
            return;
        }
        let Ok(omp_core::host::ExecutionHost::Remote(remote)) = self.execution_host() else {
            return;
        };
        self.document_state.references.loading = true;
        self.document_state.references.request = request.clone();
        let tx = self.document_state.tx.clone();
        std::thread::spawn(move || {
            fn collect(node: &markdown::mdast::Node, out: &mut std::collections::BTreeSet<String>) {
                if let markdown::mdast::Node::InlineCode(code) = node {
                    out.insert(code.value.trim().to_owned());
                }
                if let Some(children) = node.children() {
                    for child in children {
                        collect(child, out);
                    }
                }
            }
            let mut candidates = std::collections::BTreeSet::new();
            for body in bodies {
                if let Ok(node) = markdown::to_mdast(&body, &markdown::ParseOptions::gfm()) {
                    collect(&node, &mut candidates);
                }
            }
            let result = remote
                .call(omp_core::remote::protocol::Operation::ResolveReferences {
                    root,
                    candidates: candidates.into_iter().take(512).collect(),
                })
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(documents::FileEvent::References {
                session: session_id,
                request,
                result,
            });
        });
    }

    pub(in crate::desktop::workspace) fn finish_file_references(
        &mut self,
        request: String,
        result: Result<HashMap<String, PathBuf>, String>,
        cx: &mut Context<Self>,
    ) {
        if self.document_state.references.request != request {
            return;
        }
        self.document_state.references.loading = false;
        match result {
            Ok(resolved) => {
                if self.document_state.references.resolved != resolved {
                    self.document_state.references.resolved = resolved;
                    self.document_state.file_link_revision =
                        self.document_state.file_link_revision.wrapping_add(1);
                    cx.notify();
                }
            }
            Err(_) => self.document_state.references.request.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{link_matches, local_match};
    #[test]
    fn highlights_unicode_links_without_subtracting_hits_from_other_links() {
        assert_eq!(local_match(20..40, &(2..8)), None);
        assert_eq!(local_match(20..40, &(41..43)), None);
        assert_eq!(local_match(20..40, &(23..29)), Some(3..9));
        let text = "src/中文/中文 file.rs";
        let matches = link_matches(text, "中文");
        assert_eq!(matches.len(), 2);
        for range in matches {
            assert_eq!(&text[range], "中文");
        }
        assert!(link_matches(text, "").is_empty());
    }
}

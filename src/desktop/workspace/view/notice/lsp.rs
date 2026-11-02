use super::*;
use gpui_kit::base::Link;
use gpui_kit::component::{
    marker::{Marker, MarkerContent, MarkerIcon},
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast},
};
use omp_core::domain::notice::lsp::{Block, Diagnostic, Severity};

pub(super) struct LspBlocks {
    pub workspace: WeakEntity<Workspace>,
    pub session: String,
}
fn color(severity: &Severity, cx: &App) -> Hsla {
    match severity {
        Severity::Error => cx.theme().danger,
        Severity::Warning => cx.theme().warning,
        Severity::Information => cx.theme().primary,
        Severity::Hint | Severity::Unknown => cx.theme().muted_foreground,
    }
}
impl MarkdownPlugin for LspBlocks {
    fn name(&self) -> &str {
        "omp-lsp-block"
    }
    fn is_block(&self) -> bool {
        true
    }
    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        if code.lang.as_deref() != Some("omp-lsp") {
            return None;
        }
        let block: Block = serde_json::from_str(&code.value).ok()?;
        let text = block.text();
        Some(
            MarkdownNode::new(self.name(), block)
                .text(text)
                .markdown(cx.node_source(node)?.to_owned()),
        )
    }
    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = node.source_range().map_or(0, |r| r.start);
        match node
            .data::<Block>()
            .expect("LSP parser supplies typed blocks")
        {
            Block::File { path, summary } => {
                let workspace = self.workspace.clone();
                let session = self.session.clone();
                let target = path.clone();
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Link::new(("lsp-file", key))
                            .debug_selector(|| "lsp-file-path".into())
                            .accessibility_label(format!("打开文件 {path}"))
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .font_family("Cascadia Code")
                            .cursor_pointer()
                            .child(path.clone())
                            .on_activate(move |_, window, cx| {
                                if target.is_empty() {
                                    return;
                                }
                                let _ = workspace.update(cx, |this, cx| {
                                    if this.session().is_none_or(|s| s.id != session) {
                                        return;
                                    }
                                    this.open_document(PathBuf::from(&target), false);
                                    this.focus_document_editor(window, cx);
                                    cx.notify();
                                });
                            }),
                    )
                    .when(!summary.is_empty(), |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(summary.clone()),
                        )
                    })
                    .into_any_element()
            }
            Block::Diagnostic(diagnostic) => {
                diagnostic_marker(diagnostic, key, cx).into_any_element()
            }
            Block::Text { text } => div()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .child(text.clone())
                .into_any_element(),
        }
    }
}
fn diagnostic_marker(d: &Diagnostic, key: usize, cx: &App) -> Marker {
    let color = color(&d.severity, cx);
    let selector = format!(
        "lsp-status-{}",
        match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "information",
            Severity::Hint => "hint",
            Severity::Unknown => "unknown",
        }
    );
    let icon = match d.severity {
        Severity::Error => assets::IconName::CircleX,
        Severity::Warning => assets::IconName::CircleAlert,
        Severity::Information => assets::IconName::Info,
        Severity::Hint | Severity::Unknown => assets::IconName::Circle,
    };
    let metadata = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x_3()
        .gap_y_1()
        .text_xs()
        .child(div().text_color(color).child(d.severity.label()))
        .children(
            [d.location(), d.source.clone(), d.code.clone()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    div()
                        .max_w_full()
                        .text_color(cx.theme().muted_foreground)
                        .child(s)
                }),
        );
    let mut content = MarkerContent::new()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_1()
        .child(metadata)
        .when(!d.path.is_empty(), |c| {
            c.child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(d.path.clone()),
            )
        })
        .child(
            div()
                .debug_selector(|| "lsp-diagnostic-message".into())
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .child(d.message.clone()),
        );
    for related in &d.related {
        content = content.child(
            div()
                .w_full()
                .min_w_0()
                .text_xs()
                .whitespace_normal()
                .text_color(cx.theme().muted_foreground)
                .child(related.clone()),
        );
    }
    Marker::new()
        .id(("lsp-diagnostic", key))
        .w_full()
        .min_w_0()
        .items_start()
        .py_1()
        .text_color(cx.theme().foreground)
        .icon(
            MarkerIcon::new().mt(px(2.)).child(
                div()
                    .id("severity")
                    .role(Role::Image)
                    .aria_label(d.severity.label())
                    .debug_selector(move || selector.clone())
                    .child(Icon::new(icon).size_4().text_color(color)),
            ),
        )
        .content(content)
}

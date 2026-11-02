//! Formula parsing is attached to the native Markdown AST, so code examples,
//! source ranges, selection and comment anchors remain owned by TextView.
use gpui_kit::{
    component::{
        scroll::ScrollableElement,
        text::{
            InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
            markdown_ast,
        },
    },
    *,
};
use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
};

pub(super) struct MathPlugin {
    pub display: bool,
}
struct Formula {
    svg: Vec<u8>,
    width: f32,
    height: f32,
    baseline: f32,
}
type Cache = VecDeque<(String, bool, Arc<Formula>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(VecDeque::new()));
const PADDING: f64 = 0.06;

fn compile(latex: &str, display: bool) -> Option<Arc<Formula>> {
    if latex.trim().is_empty() || latex.len() > 16 * 1024 {
        return None;
    }
    {
        let mut cache = CACHE.lock().ok()?;
        if let Some(ix) = cache
            .iter()
            .position(|(s, d, _)| s == latex && *d == display)
        {
            let entry = cache.remove(ix)?;
            let result = entry.2.clone();
            cache.push_front(entry);
            return Some(result);
        }
    }
    let nodes = ratex_parser::parse(latex).ok()?;
    let options = ratex_layout::LayoutOptions {
        style: if display {
            ratex_types::math_style::MathStyle::Display
        } else {
            ratex_types::math_style::MathStyle::Text
        },
        ..Default::default()
    };
    let layout = ratex_layout::layout(&nodes, &options);
    ratex_layout::validate_layout(&layout).ok()?;
    if !layout.width.is_finite()
        || !layout.height.is_finite()
        || !layout.depth.is_finite()
        || layout.width > 512.
        || layout.height + layout.depth > 128.
    {
        return None;
    }
    let list = ratex_layout::to_display_list(&layout);
    let svg = ratex_svg::render_to_svg(
        &list,
        &ratex_svg::SvgOptions {
            font_size: 20.,
            padding: PADDING * 20.,
            embed_glyphs: true,
            ..Default::default()
        },
    )
    .into_bytes();
    let formula = Arc::new(Formula {
        svg,
        width: (list.width + 2. * PADDING) as f32,
        height: (list.height + list.depth + 2. * PADDING) as f32,
        baseline: (list.height + PADDING) as f32,
    });
    // Keep streaming reparses cheap without retaining unbounded SVG history.
    let mut cache = CACHE.lock().ok()?;
    cache.push_front((latex.to_owned(), display, formula.clone()));
    while cache.len() > 128
        || cache.iter().map(|(_, _, f)| f.svg.len()).sum::<usize>() > 4 * 1024 * 1024
    {
        cache.pop_back();
    }
    Some(formula)
}
fn image(formula: &Formula, em: f32, color: Hsla) -> impl IntoElement {
    svg()
        .data(&formula.svg)
        .w(px(formula.width * em))
        .h(px(formula.height * em))
        .flex_shrink_0()
        .debug_selector(|| "math-formula".into())
        .text_color(color)
}
impl MarkdownPlugin for MathPlugin {
    fn name(&self) -> &str {
        if self.display {
            "omp-display-math"
        } else {
            "omp-inline-math"
        }
    }
    fn is_block(&self) -> bool {
        self.display
    }
    fn parse(
        &self,
        node: &markdown_ast::Node,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let latex = match node {
            markdown_ast::Node::Math(m) if self.display => &m.value,
            markdown_ast::Node::InlineMath(m) if !self.display => &m.value,
            _ => return None,
        };
        let source = cx.node_source(node)?;
        let formula = compile(latex, self.display)?;
        Some(
            MarkdownNode::new(self.name(), formula)
                .text(source.to_owned())
                .markdown(source.to_owned())
                .accessibility_label(format!("公式：{latex}")),
        )
    }
    fn render(&self, node: &MarkdownNode, window: &mut Window, _: &mut App) -> impl IntoElement {
        let formula = node
            .data::<Arc<Formula>>()
            .expect("math parser supplies layout");
        let em: f32 = window
            .text_style()
            .font_size
            .to_pixels(window.rem_size())
            .into();
        div()
            .id(("display-math", node.source_range().map_or(0, |r| r.start)))
            .debug_selector(|| "display-math".into())
            .w_full()
            .min_w_0()
            .py_2()
            .child(image(formula, em * 1.15, window.text_style().color))
            .overflow_x_scrollbar()
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let formula = node.data::<Arc<Formula>>()?;
        let em: f32 = context.font_size().into();
        Some(
            InlineElement::new(image(formula, em, context.text_style().color))
                .with_baseline(px(formula.baseline * em)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::compile;
    #[test]
    fn typesets_common_formulas_and_reuses_prepared_outlines() {
        for formula in [
            r"\frac{1}{2}",
            r"F_{\mu\nu}=\partial_\mu A_\nu-\partial_\nu A_\mu-ig[A_\mu,A_\nu]",
            r"\int_0^\infty e^{-x^2}dx=\frac{\sqrt{\pi}}2",
            r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        ] {
            let a = compile(formula, true).expect("supported formula");
            assert!(a.width > 0. && a.height > 0. && a.baseline > 0.);
            assert!(String::from_utf8_lossy(&a.svg).contains("<path"));
            assert!(!String::from_utf8_lossy(&a.svg).contains("<text"));
            assert!(std::sync::Arc::ptr_eq(&a, &compile(formula, true).unwrap()));
        }
        assert!(compile(r"\frac{", false).is_none());
    }
}

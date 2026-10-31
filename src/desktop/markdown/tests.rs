use super::prepare;
use markdown::mdast::Node;

const SOURCE: &str = "介绍 **规范场**。\n\n## 1. 场与变换\n\n$$A_\\mu\\to U A_\\mu U^{-1},\\qquad\nD_\\mu=\\partial_\\mu-igA_\\mu.$$\n\n协变导数本身**不是**张量。\n\n## 2. 场强与作用量\n\n$$S=-\\frac14\\int F^2\n+\\bar\\psi\\psi.$$\n\n## 3. 后续章节\n\n- **非线性**\n- 另一项\n";

fn native_ast(source: &str) -> Node {
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_flow = true;
    options.constructs.math_text = true;
    markdown::to_mdast(source, &options).unwrap()
}

#[test]
fn compact_multiline_math_closes_before_following_headings_and_lists() {
    let old = native_ast(SOURCE);
    assert_eq!(
        old.children()
            .unwrap()
            .iter()
            .filter(|n| matches!(n, Node::Heading(_)))
            .count(),
        1,
        "reproduce the native parser swallowing subsequent sections"
    );
    let (display, _) = prepare(SOURCE);
    let ast = native_ast(&display);
    let blocks = ast.children().unwrap();
    assert_eq!(
        blocks
            .iter()
            .filter(|n| matches!(n, Node::Heading(_)))
            .count(),
        3
    );
    assert_eq!(
        blocks.iter().filter(|n| matches!(n, Node::List(_))).count(),
        1
    );
    let math = blocks
        .iter()
        .filter_map(|n| match n {
            Node::Math(m) => Some(m),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(math.len(), 2);
    assert!(
        math[0].value.starts_with("A_\\mu"),
        "formula first line must not become metadata"
    );
    assert!(
        math.iter()
            .all(|m| m.meta.is_none() && !m.value.contains("##"))
    );
    assert_eq!(
        prepare(&display).0.as_ref(),
        display.as_ref(),
        "normalization is idempotent"
    );
}

#[test]
fn comments_map_unicode_and_formula_ranges_back_to_unchanged_source() {
    let (display, offsets) = prepare(SOURCE);
    assert!(offsets.matches(&display, SOURCE));
    assert!(
        !offsets.matches(SOURCE, SOURCE),
        "reject the previous parse while normalization is pending"
    );
    assert!(!offsets.matches(&display.replace("场强", "张量"), SOURCE));
    for quote in [
        "介绍",
        "A_\\mu",
        "D_\\mu",
        "协变导数",
        "## 2. 场强与作用量",
        "**非线性**",
    ] {
        let start = SOURCE.find(quote).unwrap();
        let original = start..start + quote.len();
        let mapped = offsets.display_range(original.clone());
        assert_eq!(&display[mapped.clone()], quote);
        assert_eq!(offsets.original_range(mapped), original);
    }
    for (end, _) in SOURCE
        .char_indices()
        .chain(std::iter::once((SOURCE.len(), '\0')))
    {
        let (partial, mapping) = prepare(&SOURCE[..end]);
        assert_eq!(mapping.original_range(0..partial.len()), 0..end);
    }
}

#[test]
fn preserves_real_code_escaped_dollars_and_existing_canonical_fences() {
    for source in [
        "```text\n$$A\nB.$$\n## literal heading\n```\n\n## Real heading",
        "    $$A\n    B.$$\n\n## Heading",
        "`$$A\nB.$$`\n\n## Heading",
        "\\$$A\nB.$$\n\n## Heading",
        "$$\nA\nB\n$$\n\n## Heading",
        "$$A+B$$\n\n## Heading",
        "Cost $5, inline $G=SU(N)$, and `path.rs`.",
    ] {
        assert_eq!(prepare(source).0.as_ref(), source);
    }
}

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
    VisualTestContext, Window, WindowOptions,
    component::text::{TextView, TextViewState},
    div, open_window,
};
struct Fixture {
    text: Entity<TextViewState>,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(TextView::new(&self.text))
    }
}

#[gpui_kit::test]
fn native_text_view_renders_following_markdown_and_retains_source_mapping(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                let (display, _) = prepare(SOURCE);
                Fixture {
                    text: cx.new(|cx| TextViewState::markdown(&display, cx).selectable(true)),
                }
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    view.update(&mut visual, |fixture, cx| {
        let rendered = fixture.text.read(cx).rendered_text();
        assert!(rendered.as_str().contains("A_\\mu"));
        assert!(rendered.as_str().contains("2. 场强与作用量"));
        assert!(!rendered.as_str().contains("## 2."));
        assert!(!rendered.as_str().contains("**非线性**"));
        let (_, offsets) = prepare(SOURCE);
        for quote in ["A_\\mu", "D_\\mu", "协变导数", "场强与作用量"] {
            let start = SOURCE.find(quote).unwrap();
            let range = rendered
                .range_for_source(offsets.display_range(start..start + quote.len()))
                .unwrap();
            assert_eq!(&rendered.as_str()[range], quote);
        }
    });
}

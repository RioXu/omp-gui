use super::selection_guard;
use gpui_kit::{
    AppContext, Bounds, Context, Entity, IntoElement, Modifiers, MouseButton, Render,
    TestAppContext, VisualTestContext, Window, WindowBounds, WindowOptions,
    base::{
        TextSelection, TextViewStyle,
        text::{TextView, TextViewState},
    },
    div, point,
    prelude::*,
    px, size,
};

const TEXT: &str = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
const TEXT_BACKGROUND: u32 = 0x12abcd;

struct SelectionFixture {
    text: Entity<TextViewState>,
}

impl Render for SelectionFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(selection_guard(|_, _| {})).child(
            TextView::new(&self.text).selectable(true).style(
                TextViewStyle::default().with_inline_code(gpui_kit::HighlightStyle {
                    background_color: Some(gpui_kit::rgb(TEXT_BACKGROUND).into()),
                    ..Default::default()
                }),
            ),
        )
    }
}

#[gpui_kit::test]
fn missed_mouse_up_stops_on_hover_and_keeps_the_completed_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(240.)));
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|cx| SelectionFixture {
                    text: cx.new(|cx| TextViewState::markdown(&format!("`{TEXT}`"), cx)),
                })
            },
        )
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(window, cx);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.update(|window, _| {
        let background = gpui_kit::Background::from(gpui_kit::rgb(TEXT_BACKGROUND));
        let scale = window.scale_factor();
        window
            .painted_quads()
            .into_iter()
            .find(|q| q.background == background)
            .unwrap()
            .bounds
            .map(|value| px(value.0 / scale))
    });
    let start = point(bounds.left() + px(12.), bounds.center().y);
    let end = point(bounds.left() + px(100.), bounds.center().y);
    let hover = point(bounds.left() + px(300.), bounds.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let selected = cx.update(TextSelection::selected_text);
    assert!(!selected.is_empty());
    assert!(view.read_with(&cx, |v, cx| v.text.read(cx).is_selecting()));

    // No MouseUp is delivered: the next event is ordinary pointer movement.
    cx.simulate_mouse_move(hover, None, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert!(!view.read_with(&cx, |v, cx| v.text.read(cx).is_selecting()));
    assert_eq!(cx.update(TextSelection::selected_text), selected);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(cx.update(TextSelection::selected_text), selected);

    // A new real drag still extends normally, and release preserves its range.
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_mouse_move(hover, Some(MouseButton::Left), Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    let extended = cx.update(TextSelection::selected_text);
    assert_ne!(extended, selected);
    cx.simulate_mouse_up(hover, MouseButton::Left, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    cx.simulate_mouse_move(end, None, Modifiers::default());
    cx.update(|w, cx| w.draw(cx).clear(cx));
    assert_eq!(cx.update(TextSelection::selected_text), extended);
}

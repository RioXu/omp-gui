use gpui_kit::{base::TextSelection, *};
use std::rc::Rc;

#[cfg(test)]
mod tests;

/// gpui-kit 0.8.1 continues a window selection on bare pointer movement after
/// a missed mouse-up. End it during capture, before its bubble-phase handler
/// can move the endpoint. `end` preserves the selected text and stops scrolling.
pub(crate) fn selection_guard(
    on_finish: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_finish = Rc::new(on_finish);
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let on_move = on_finish.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase.capture() && event.pressed_button != Some(MouseButton::Left) {
                    let had_selection = TextSelection::has_selection(window, cx);
                    TextSelection::end(window, cx);
                    if had_selection {
                        on_move(window, cx);
                    }
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase.capture() && event.button == MouseButton::Left {
                    TextSelection::end(window, cx);
                    on_finish(window, cx);
                }
            });
        },
    )
    .size_0()
}

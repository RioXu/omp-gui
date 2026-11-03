use crate::{
    desktop::workspace::runtime::sessions::tests::fixture,
    domain::{Message, process_blocks},
};
use gpui_kit::{
    AppContext, Modifiers, TestAppContext, VisualTestContext, WindowOptions, open_window, px, size,
};
use serde_json::json;

#[gpui_kit::test]
fn thinking_follows_the_last_five_wrapped_lines_and_can_collapse_while_streaming(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut message = Message::new("assistant", String::new());
    message.process = process_blocks(
        &json!({"content":[{"type":"thinking","thinking":"Paragraph **bold** with a soft\nline break.\n\n".repeat(12)}]}),
        "message_update",
        "thinking_delta",
    );
    let id = message.id.clone();
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                this.db.sessions[0].messages = vec![message];
                this.runtime.running = true;
                this.panel_open = false;
                this.sync_texts(cx);
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1100.), px(850.)));
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let preview = visual.debug_bounds("thinking-preview").unwrap();
    let text = visual.debug_bounds("thinking-preview-text").unwrap();
    assert!(preview.size.height <= px(120.) && preview.size.height >= px(96.));
    assert!(text.size.height > preview.size.height);
    assert!(
        (text.bottom() - preview.bottom()).abs() < px(1.),
        "preview must show the end of the text: text={text:?}, preview={preview:?}"
    );
    let original_preview_height = preview.size.height;
    let button = visual.debug_bounds("thinking-toggle").unwrap();
    let card = visual.debug_bounds("thinking-card").unwrap();
    assert!((button.left() - card.left()).abs() < px(1.));
    assert!(button.size.width < card.size.width / 2.);
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.db.sessions[0].messages[0].process[0].text =
                "New streaming tail with 中文 and wrapping. ".repeat(80);
            this.invalidate_list();
            cx.notify();
        })
    });
    visual.simulate_resize(size(px(760.), px(850.)));
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let preview = visual.debug_bounds("thinking-preview").unwrap();
    let text = visual.debug_bounds("thinking-preview-text").unwrap();
    assert_eq!(preview.size.height, original_preview_height);
    let preview_text_size = text.size;
    assert!(text.size.height > preview.size.height);
    assert!((text.bottom() - preview.bottom()).abs() < px(1.));
    let button = visual.debug_bounds("thinking-toggle").unwrap();
    visual.simulate_click(button.center(), Modifiers::default());
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("thinking-preview").is_none());
    assert_eq!(
        visual.debug_bounds("thinking-preview-text").unwrap().size,
        preview_text_size,
        "preview and full thinking must share Markdown layout"
    );
    assert!(view.read_with(&visual, |this, _| {
        this.runtime
            .expanded_process
            .contains(&format!("process-{id}-thinking-0"))
    }));
    let button = visual.debug_bounds("thinking-toggle").unwrap();
    visual.simulate_click(button.center(), Modifiers::default());
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("thinking-preview").unwrap().size.height <= px(120.));
    assert!(view.read_with(&visual, |this, _| {
        this.db.sessions[0].messages[0].process[0].streaming
    }));
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.db.sessions[0].messages[0].process[0].streaming = false;
            this.invalidate_list();
            cx.notify();
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        visual.debug_bounds("thinking-preview").is_none(),
        "completed thinking has no preview"
    );
    assert!(visual.debug_bounds("thinking-full").is_none());
    let button = visual.debug_bounds("thinking-toggle").unwrap();
    visual.simulate_click(button.center(), Modifiers::default());
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("thinking-full").unwrap().size.height > px(120.));
}

use super::*;
use crate::desktop::workspace::runtime::sessions::tests::fixture;
use core::prelude::v1::test;
use gpui_kit::{TestAppContext, VisualTestContext};

#[gpui_kit::test]
fn thousand_steps_scroll_without_paging_and_only_visible_rows_are_built(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| open_window(WindowOptions::default(), cx, |window, cx| cx.new(|cx| {
        let mut this = fixture(window, cx);
        let mut message = Message::new("assistant", String::new());
        let calls: Vec<_> = (0..1000).map(|ix| json!({"type":"toolCall","id":format!("read-{ix}"),"name":"read","arguments":{"path":"file.rs"}})).collect();
        message.process = process_blocks(&json!({"content":calls}), "message_end", "");
        message.id = "large".into();
        this.db.sessions[0].messages = vec![message];
        this.runtime.expanded_process.insert("turn-large".into());
        this.panel_open = false;
        this.sync_texts(cx);
        this
    })).unwrap());
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("process-read-large-read-0").is_some());
    assert!(visual.debug_bounds("process-read-large-read-500").is_none());
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            let list = this.runtime.process_lists.borrow()["turn-large"].clone();
            assert_eq!(list.item_count(), 1000);
            list.scroll_to(ListOffset {
                item_ix: 500,
                offset_in_item: px(0.),
            });
            cx.notify();
        })
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("process-read-large-read-0").is_none());
    assert!(visual.debug_bounds("process-read-large-read-500").is_some());
    view.read_with(&visual, |this, _| {
        assert_eq!(this.db.sessions[0].messages[0].process.len(), 1000)
    });
}

#[gpui_kit::test]
fn long_tool_output_keeps_full_text_and_virtualizes_editor_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = (0..10000)
        .map(|ix| format!("line {ix} 中文"))
        .collect::<Vec<_>>()
        .join(&char::from(10).to_string());
    let (window, view) = cx.update(|cx| open_window(WindowOptions::default(), cx, |window, cx| cx.new(|cx| {
        let mut this = fixture(window, cx);
        let mut message = Message::new("assistant", String::new());
        message.id = "long".into();
        message.process = process_blocks(&json!({"content":[{"type":"toolCall","id":"out","name":"bash","arguments":{"command":"echo"}}]}), "message_end", "");
        message.process[0].result = Some(text.clone());
        this.db.sessions[0].messages = vec![message];
        this.runtime.expanded_process.extend(["turn-long".into(), "process-long-out".into()]);
        this.panel_open = false;
        this.sync_texts(cx);
        this
    })).unwrap());
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let state = view.read_with(&visual, |this, cx| {
        let cache = this.runtime.process_texts.borrow();
        let (source, state) = &cache["output-process-long-out"];
        assert_eq!(source, &text);
        assert_eq!(state.read(cx).value().as_str(), text);
        let visible = state.read(cx).visible_row_range().unwrap();
        assert!(visible.len() < 100);
        state.clone()
    });
    visual.update(|window, cx| {
        state.update(cx, |state, cx| {
            state.set_scroll_offset(point(px(0.), px(-10000.)), cx)
        });
        state.read(cx).focus_handle(cx).focus(window, cx);
    });
    visual.simulate_input("MUST NOT EDIT");
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    visual.update(|_, cx| {
        assert_eq!(state.read(cx).value().as_str(), text);
        let visible = state.read(cx).visible_row_range().unwrap();
        assert!(visible.start > 0 && visible.len() < 100);
        assert_eq!(
            view.read(cx).runtime.process_texts.borrow()["output-process-long-out"].1,
            state
        );
    });
}

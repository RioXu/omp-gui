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
    assert!(preview.size.height <= px(105.) && preview.size.height >= px(84.));
    assert!(text.size.height > preview.size.height);
    assert!(
        (text.bottom() - preview.bottom()).abs() < px(1.),
        "preview must show the end of the text: text={text:?}, preview={preview:?}"
    );
    let original_preview_height = preview.size.height;
    let button = visual.debug_bounds("thinking-toggle").unwrap();
    let card = visual.debug_bounds("thinking-card").unwrap();
    assert!(
        button.left() > card.left(),
        "toggle sits inside the border padding"
    );
    assert!(text.left() > card.left() && text.right() < card.right());
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
    assert!(visual.debug_bounds("thinking-preview").unwrap().size.height <= px(105.));
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

#[gpui_kit::test]
fn search_finds_collapsed_thinking_in_main_and_child_tabs(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut message = Message::new("assistant", "最终答案不含查找词".into());
    message.process = process_blocks(
        &json!({"content":[{"type":"thinking","thinking":"前言\n\n**独有思考词** 和中文。"}]}),
        "message_end",
        "",
    );
    let calls: Vec<_> = (0..100).map(|ix| json!({"type":"toolCall","id":format!("read-{ix}"),"name":"read","arguments":{"path":"file.rs"}})).collect();
    message
        .process
        .extend(process_blocks(&json!({"content":calls}), "message_end", ""));
    let message_id = message.id.clone();
    let block_id = message.process[0].id.clone();
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                this.db.sessions[0].messages = vec![message.clone()];
                this.panel_open = false;
                this.sync_texts(cx);
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1100.), px(850.)));
    for child in [false, true] {
        visual.update(|window, cx| {
            view.update(cx, |this, cx| {
                if child {
                    this.runtime.agents = vec![
                        json!({"id":"child","role":"reviewer","title":"检查","status":"completed"}),
                    ];
                    this.open_subagent("child".into());
                    this.runtime.collaboration.messages = vec![message.clone()];
                    this.sync_subagent_view(cx);
                }
                this.runtime.expanded_process.clear();
                this.search_state
                    .input
                    .update(cx, |input, cx| input.set_value("独有思考词", window, cx));
                this.open_search(window, cx);
            })
        });
        visual.run_until_parked();
        visual.update(|_, cx| view.update(cx, |this, cx| this.update_search(false, cx)));
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        let key = format!(
            "process-{}{message_id}-{block_id}",
            if child { "child-" } else { "" }
        );
        view.read_with(&visual, |this, cx| {
            assert_eq!(this.search_state.hits.len(), 1);
            assert_eq!(this.search_state.hits[0].0, key);
            assert!(this.runtime.expanded_process.contains(&key));
            let rendered = this.search_state.thinking[&key]
                .state
                .read(cx)
                .rendered_text();
            assert_eq!(
                &rendered.as_str()[this.search_state.hits[0].1.clone()],
                "独有思考词"
            );
        });
        assert!(
            visual.debug_bounds("thinking-full").is_some(),
            "search must open both outer process and inner thinking"
        );
        // A match arriving from streaming updates must not expand or reveal itself.
        visual.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.search_state
                    .input
                    .update(cx, |input, cx| input.set_value("新增思考词", window, cx));
                this.update_search(true, cx);
                assert!(this.search_state.hits.is_empty());
                this.runtime.expanded_process.clear();
                if child {
                    this.runtime.collaboration.messages[0].process[0]
                        .text
                        .push_str(" 新增思考词");
                    this.sync_subagent_view(cx);
                } else {
                    this.db.sessions[0].messages[0].process[0]
                        .text
                        .push_str(" 新增思考词");
                    this.runtime.text_updates.insert(0);
                    this.sync_updated_texts(cx);
                }
            })
        });
        visual.run_until_parked();
        visual.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.update_search(false, cx);
                assert_eq!(this.search_state.hits.len(), 1);
                assert!(!this.runtime.expanded_process.contains(&key));
                this.move_search(false, cx);
                assert!(this.runtime.expanded_process.contains(&key));
                this.move_search(true, cx);
                assert_eq!(this.search_state.index, 0);
            })
        });
        visual.update(|window, cx| view.update(cx, |this, cx| this.close_search(window, cx)));
        view.read_with(&visual, |this, _| {
            assert!(this.search_state.thinking.is_empty())
        });
    }
}

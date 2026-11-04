use super::tests::fixture;
use super::*;
use core::prelude::v1::test;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn body_stream_preserves_reading_position_and_completion_reveals_the_end(cx: &mut TestAppContext) {
    use gpui_kit::{VisualTestContext, size};
    cx.update(gpui_kit::init);
    let initial = "A paragraph of model output to read.\n\n".repeat(80);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                this.panel_open = false;
                this.db.sessions[0].messages = vec![Message::new("assistant", initial.clone())];
                this.sync_texts(cx);
                let id = this.db.sessions[0].messages[0].id.clone();
                this.runtime.rpc_messages.insert("live".into(), id);
                this.runtime.running = true;
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_resize(size(px(1100.), px(850.)));
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    // No selection is required to read. Starting at the measured bottom must
    // not leave an implicit end anchor that moves on every model token.
    let (mut position, focus, session) = visual.update(|window, cx| {
        let focus = view.read(cx).runtime.composer_state.input.focus_handle(cx);
        focus.focus(window, cx);
        let this = view.read(cx);
        (
            this.runtime.list.scroll_px_offset_for_scrollbar(),
            window.focused(cx),
            this.active,
        )
    });
    for frame in [
        json!({"type":"message_update","messageId":"live","message":{"role":"assistant","content":[{"type":"text","text":format!("{initial}{}", "More streamed output.\n\n".repeat(30))}]}}),
        json!({"type":"message_end","messageId":"live","message":{"role":"assistant","content":[{"type":"text","text":format!("{initial}{}", "More streamed output.\n\n".repeat(60))}]}}),
        json!({"type":"agent_end"}),
        json!({"type":"session_settled"}),
        json!({"type":"extension_ui_request","method":"input","id":"question","title":"What next?"}),
    ] {
        visual.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.on_frame(frame.clone(), window, cx);
                this.poll_engine(window, cx);
                cx.notify();
            })
        });
        // Assert after asynchronous Markdown parsing and list layout, not just
        // immediately after event dispatch.
        visual.run_until_parked();
        visual.update(|window, cx| {
            window.draw(cx).clear(cx);
            let this = view.read(cx);
            if matches!(
                frame["type"].as_str(),
                Some("agent_end" | "session_settled" | "extension_ui_request")
            ) {
                assert_eq!(this.runtime.list.is_scrolled_to_end(), Some(true));
            } else {
                assert_eq!(
                    this.runtime.list.scroll_px_offset_for_scrollbar(),
                    position,
                    "{} moved the reader",
                    frame["type"]
                );
            }
            assert_eq!(window.focused(cx), focus);
            assert_eq!(this.active, session);
        });
        if frame["type"] == "message_update" || frame["type"] == "session_settled" {
            // Streaming preserves a reader's position; the event boundary
            // deliberately reveals the final output even after scrolling up.
            visual.update(|_, cx| {
                view.update(cx, |this, cx| {
                    this.runtime.list.scroll_by(px(-400.));
                    cx.notify();
                })
            });
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            position = view.read_with(&visual, |this, _| {
                this.runtime.list.scroll_px_offset_for_scrollbar()
            });
        }
    }
}

#[gpui_kit::test]
fn settling_does_not_collapse_the_process_being_read(cx: &mut TestAppContext) {
    use gpui_kit::{VisualTestContext, size};
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                let mut message = Message::new("assistant", "The answer.\n\n".repeat(10));
                let content: Vec<_> = (0..24).map(|i| json!({"type":"toolCall","id":format!("tool-{i}"),"name":"hub","arguments":{}}))
                    .chain(std::iter::once(json!({"type":"thinking","thinking":"Thinking about the next step.\n\n".repeat(30)})))
                    .collect();
                message.process = process_blocks(&json!({"content":content}), "message_update", "thinking_delta");
                this.db.sessions[0].messages = vec![message];
                this.runtime.running = true;
                this.panel_open = false;
                this.sync_texts(cx);
                this
            })
        }).unwrap()
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_resize(size(px(1100.), px(850.)));
    visual.run_until_parked();
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.runtime.list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(600.),
            });
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let expanded = view.read_with(&visual, |this, _| this.runtime.expanded_process.clone());
    let previews = view.read_with(&visual, |this, _| this.runtime.thinking_previews.clone());
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.on_frame(json!({"type":"session_settled"}), window, cx);
            this.poll_engine(window, cx);
            cx.notify();
        })
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(&visual, |this, _| {
        assert!(expanded.is_subset(&this.runtime.expanded_process));
        assert!(previews.is_subset(&this.runtime.thinking_previews));
        assert!(
            this.runtime
                .expanded_process
                .contains(&format!("turn-{}", this.db.sessions[0].messages[0].id))
        );
        assert_eq!(this.runtime.list.is_scrolled_to_end(), Some(true));
    });
}

#[gpui_kit::test]
fn first_streamed_search_match_keeps_keyboard_focus_and_reading_position(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle.update(cx, |_, window, cx| view.update(cx, |this,cx| {
        for _ in 0..30 { this.db.sessions[0].messages.push(Message::new("user", "earlier history".into())); }
        this.sync_texts(cx);
        this.search_state.open = true;
        this.search_state.input.update(cx, |input,cx| input.set_value("NEW_MATCH", window,cx));
        this.update_search(true,cx);
        assert!(this.search_state.hits.is_empty());
        this.runtime.list.scroll_to(ListOffset { item_ix: 10, offset_in_item: px(7.) });
        let focus = this.search_state.input.focus_handle(cx);
        focus.focus(window,cx);
        let draft = this.runtime.composer_state.input.read(cx).value();
        this.on_frame(json!({"type":"message_update","messageId":"live","message":{"role":"assistant","content":[{"type":"text","text":"NEW_MATCH in newly generated output"}]}}), window,cx);
        this.poll_engine(window,cx);
        this.update_search(false,cx);
        assert!(!this.search_state.hits.is_empty(), "the stream must actually introduce a first search match");
        assert_eq!(this.runtime.list.logical_scroll_top(), ListOffset { item_ix: 10, offset_in_item: px(7.) });
        assert_eq!(window.focused(cx), Some(focus));
        assert_eq!(this.runtime.composer_state.input.read(cx).value(), draft);
    })).unwrap();
}

#[gpui_kit::test]
fn stream_updates_reuse_history_entities_and_keep_the_scroll_anchor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle.update(cx, |_, window, cx| view.update(cx, |this, cx| {
        for _ in 0..100 { this.db.sessions[0].messages.push(Message::new("user", "history".into())); }
        this.sync_texts(cx);
        let first_id = this.db.sessions[0].messages[0].id.clone();
        let entity = this.runtime.texts[&first_id].state.entity_id();
        this.runtime.list.scroll_to(ListOffset { item_ix: 10, offset_in_item: px(7.) });
        for text in ["first", "second", "final"] {
            this.on_frame(json!({"type":"message_update", "messageId":"stream", "message":{"role":"assistant", "content":[{"type":"text","text":text}]}}), window, cx);
        }
        assert!(!this.runtime.text_update_pending);
        assert_eq!(this.runtime.text_updates.len(), 1);
        this.poll_engine(window, cx);
        assert_eq!(this.runtime.transcript, transcript_rows(&this.db.sessions[0].messages));
        assert_eq!(this.runtime.texts[&first_id].state.entity_id(), entity);
        let last_id = this.db.sessions[0].messages.last().unwrap().id.clone();
        assert_eq!(this.runtime.texts[&last_id].body, "final");
        let offset = this.runtime.list.logical_scroll_top();
        assert_eq!(offset.item_ix, 10); assert_eq!(offset.offset_in_item, px(7.));
        this.on_frame(json!({"type":"message_update", "messageId":"stream", "message":{"role":"assistant", "content":[{"type":"text","text":"more"}]}}), window, cx);
        this.poll_engine(window, cx);
        assert_eq!(this.runtime.transcript, transcript_rows(&this.db.sessions[0].messages));
        assert_eq!(this.runtime.texts[&last_id].body, "more");
    })).unwrap();
}

#[gpui_kit::test]
fn idle_cache_is_bounded_and_preserves_busy_sessions_release_receipts_and_drafts(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |this, cx| {
                for index in 0..8 {
                    let mut session = Session::new("fixture".into());
                    session.draft = format!("draft {index}");
                    let mut state = Workspace::new_session_runtime(
                        window,
                        cx,
                        true,
                        session.draft.clone(),
                        None,
                    );
                    state.cached_at = Instant::now() - Duration::from_secs(20 - index);
                    this.background.insert(session.id.clone(), state);
                    this.db.sessions.push(session);
                }
                let evicted = this.db.sessions[2].id.clone();
                let busy = this.db.sessions[3].id.clone();
                this.background.get_mut(&busy).unwrap().running = true;
                let releasing = this.db.sessions[4].id.clone();
                let (tx, rx) = mpsc::channel();
                this.background.get_mut(&releasing).unwrap().releasing = Some(rx);
                this.trim_session_cache();
                assert!(!this.background.contains_key(&evicted));
                assert!(this.background.contains_key(&busy));
                assert!(this.background.contains_key(&releasing));
                assert!(this.background.len() <= 5);
                this.activate(2, window, cx);
                assert_eq!(
                    this.runtime.composer_state.input.read(cx).value().as_ref(),
                    "draft 0"
                );
                let state = this
                    .background
                    .values_mut()
                    .find(|s| !s.running && s.releasing.is_none())
                    .unwrap();
                state.texts.insert(
                    "large".into(),
                    TextEntry {
                        body: "x".repeat(9 * 1024 * 1024),
                        source: Source::Message("large".into()),
                        offsets: Default::default(),
                        state: cx.new(|cx| TextViewState::markdown("", cx)),
                    },
                );
                this.trim_session_cache();
                assert!(
                    this.background
                        .values()
                        .all(|s| s.cache_bytes() <= 32 * 1024 * 1024)
                );
                assert!(this.background[&busy].needs_poll());
                drop(tx);
            })
        })
        .unwrap();
}

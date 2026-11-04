use super::tests::fixture;
use super::*;
use core::prelude::v1::test;
use gpui_kit::{SystemNotificationResponse, TestAppContext, VisualTestContext};

#[gpui_kit::test]
fn notifications_require_an_inactive_window_and_deduplicate_event_replays(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_app_identity("RioXu.OMP.GUI", "OMP GUI");
    });
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    visual.update(|window, cx| {
        assert!(window.is_window_active());
        view.update(cx, |this, cx| {
            this.on_frame(json!({"type":"agent_start"}), window, cx);
            this.on_frame(json!({"type":"agent_end"}), window, cx);
            this.on_frame(json!({"type":"session_settled"}), window, cx);
        });
    });
    assert!(visual.shown_system_notifications().is_empty());
    visual.deactivate_window();
    visual.update(|window, cx| view.update(cx, |this, cx| {
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        this.on_frame(json!({"type":"agent_end"}), window, cx);
        // A state response may arrive before the settlement event.
        this.on_frame(json!({"type":"response","command":"get_state","success":true,"data":{"isSettled":true}}), window, cx);
        this.on_frame(json!({"type":"session_settled"}), window, cx);
        this.on_frame(json!({"type":"prompt_result","sessionSettled":true}), window, cx);
        this.on_frame(json!({"type":"session_settled"}), window, cx);
    }));
    assert_eq!(visual.shown_system_notifications().len(), 1);
    assert_eq!(visual.shown_system_notifications()[0].title, "会话完成");
    visual.update(|window, cx| view.update(cx, |this, cx| {
        let request = json!({"type":"extension_ui_request","method":"select","id":"choice","title":"Choose","options":["A","B"]});
        this.on_frame(request.clone(), window, cx);
        this.on_frame(request, window, cx);
        this.on_frame(json!({"type":"extension_ui_request","method":"input","secret":true,"id":"secret"}), window, cx);
        this.poll_engine(window, cx);
        assert_eq!(this.runtime.requests.len(), 1);
        assert_eq!(this.runtime.list.logical_scroll_top().item_ix, this.runtime.list.item_count());
    }));
    assert_eq!(visual.shown_system_notifications().len(), 2);
    assert_eq!(visual.shown_system_notifications()[1].title, "会话待选择");
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.on_frame(
                json!({"type":"extension_ui_request","method":"cancel","targetId":"choice"}),
                window,
                cx,
            );
            this.on_frame(json!({"type":"agent_start"}), window, cx);
            this.on_frame(
                json!({"type":"session_settled","status":"aborted"}),
                window,
                cx,
            );
        })
    });
    assert_eq!(visual.shown_system_notifications().len(), 2);
}

#[gpui_kit::test]
fn background_attention_preserves_visible_session_and_notification_opens_its_owner(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_app_identity("RioXu.OMP.GUI", "OMP GUI");
    });
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut this = fixture(window, cx);
                this.db.sessions[0].messages[0].text = "Long conversation history.\n\n".repeat(200);
                this.sync_texts(cx);
                this
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.simulate_resize(gpui_kit::size(px(1100.), px(850.)));
    visual.run_until_parked();
    visual.deactivate_window();
    let owner = visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            let owner = this.db.sessions[0].id.clone();
            this.activate(1, window, cx);
            let visible = this.active;
            let position = this.runtime.list.logical_scroll_top();
            this.dispatch_background(&owner, window, cx, |this, window, cx| {
                this.on_frame(
                    json!({"type":"extension_ui_request","method":"confirm","id":"confirm"}),
                    window,
                    cx,
                );
                this.poll_engine(window, cx);
            });
            assert_eq!(this.active, visible);
            assert_eq!(this.runtime.list.logical_scroll_top(), position);
            assert!(this.background[&owner].reveal_tail);
            owner
        })
    });
    let notification = visual.shown_system_notifications().pop().unwrap();
    assert!(notification.body.contains("A"));
    visual.simulate_system_notification_response(SystemNotificationResponse {
        tag: notification.tag,
        action_id: None,
    });
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(&visual, |this, _| {
        assert_eq!(this.session().unwrap().id, owner);
        assert_eq!(this.runtime.requests.len(), 1);
        assert_eq!(this.runtime.list.is_scrolled_to_end(), Some(true));
    });
}

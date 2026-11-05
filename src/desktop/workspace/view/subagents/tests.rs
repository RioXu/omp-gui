use crate::desktop::workspace::runtime::sessions::tests::fixture;
use gpui_kit::{
    AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowOptions,
    open_window, px, size,
};
use serde_json::json;

fn draw(visual: &mut VisualTestContext) {
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(visual: &mut VisualTestContext, selector: &'static str) {
    let bounds = visual.debug_bounds(selector).unwrap();
    visual.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    visual.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
    draw(visual);
}

#[gpui_kit::test]
fn compact_subagent_rows_open_reusable_central_tabs_and_close_independently(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| cx.new(|cx| {
            let mut this = fixture(window, cx);
            this.panel_open = true;
            this.panel_tab = "子智能体";
            this.runtime.agents = vec![
                json!({"id":"a","role":"reviewer","title":"检查实现","status":"running","task":"Long task\n".repeat(100),"output":"Long result\n".repeat(100),"progress":{"currentTool":"read"}}),
                json!({"id":"b","agent":"scout","description":"查找代码","status":"completed","output":"Result B"}),
            ];
            this
        })).unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1240.), px(850.)));
    draw(&mut visual);
    for selector in ["subagent-row-a", "subagent-row-b"] {
        let row = visual.debug_bounds(selector).unwrap();
        assert!(
            row.size.height <= px(72.),
            "list must not include task or result bodies"
        );
    }
    click(&mut visual, "subagent-row-a");
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            use crate::domain::Message;
            this.runtime.collaboration.messages = vec![
                Message::new("user", "检查这个实现".into()),
                Message::new("assistant", "检查过程".into()),
                Message::new("assistant", "## 检查结果\n\n实现已经完成。".into()),
            ];
            this.sync_subagent_view(cx);
            cx.notify();
        })
    });
    draw(&mut visual);
    assert!(visual.debug_bounds("subagent-tab-a").is_some());
    let transcript = visual.debug_bounds("subagent-transcript-a").unwrap();
    let message = visual.debug_bounds("child-message-body").unwrap();
    assert!(message.left() > transcript.left());
    assert!(message.right() < transcript.right());
    let row = visual.debug_bounds("subagent-row-a").unwrap();
    assert!(
        transcript.right() < row.left(),
        "history belongs to the central tab, not the panel"
    );
    click(&mut visual, "subagent-row-a");
    assert_eq!(
        view.read_with(&visual, |this, _| this.runtime.collaboration.tabs.len()),
        1
    );
    click(&mut visual, "subagent-row-b");
    view.read_with(&visual, |this, _| {
        assert_eq!(this.active_subagent(), Some("b"));
        assert_eq!(this.runtime.collaboration.tabs, vec!["a", "b"]);
        assert!(
            this.document_state.tabs.is_empty(),
            "child tabs must not be opened as files"
        );
        assert!(this.document_state.path.is_none());
    });
    click(&mut visual, "subagent-tab-a");
    assert!(visual.debug_bounds("subagent-transcript-a").is_some());
    click(&mut visual, "close-subagent-tab-a");
    assert!(visual.debug_bounds("subagent-tab-a").is_none());
    assert!(visual.debug_bounds("subagent-transcript-b").is_some());
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.panel_open = false;
            cx.notify();
        })
    });
    draw(&mut visual);
    assert!(visual.debug_bounds("subagent-transcript-b").is_some());
    click(&mut visual, "close-subagent-tab-b");
    view.read_with(&visual, |this, _| {
        assert!(this.runtime.collaboration.tabs.is_empty());
        assert!(this.document_state.active.is_none());
    });
}

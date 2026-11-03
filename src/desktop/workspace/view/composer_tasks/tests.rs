use crate::desktop::workspace::runtime::sessions::tests::fixture;
use gpui_kit::{
    AppContext, Modifiers, TestAppContext, VisualTestContext, WindowOptions,
    component::{Theme, ThemeMode},
    open_window, px, size,
};
use serde_json::json;

#[gpui_kit::test]
fn tasks_expand_above_the_composer_without_narrowing_chat_or_overlapping_intent(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let (window, view) = cx.update(|cx| open_window(WindowOptions::default(), cx, |window,cx| cx.new(|cx| {
        let mut this = fixture(window, cx);
        this.panel_open = false;
        this.runtime.running = true;
        this.runtime.working_intent = Some("检查任务与模型摘要在窄窗口中的显示 ".repeat(40));
        this.runtime.todos = vec![json!({"name":"验证阶段","tasks":(0..25).map(|i|json!({"content":format!("任务 {i}：验证长文字自动换行与状态标记"),"status":if i == 0 {"completed"} else {"in_progress"},"details":"保留现有任务展示样式"})).collect::<Vec<_>>()})];
        this
    })).unwrap());
    let mut visual = VisualTestContext::from_window(window, cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        visual.update(|window, cx| Theme::change(mode, Some(window), cx));
        for width in [850., 1500.] {
            visual.simulate_resize(size(px(width), px(850.)));
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let input = visual.debug_bounds("composer-input-card").unwrap();
            let intent = visual.debug_bounds("working-intent").unwrap();
            let toggle = visual.debug_bounds("composer-tasks-toggle").unwrap();
            let context = visual.debug_bounds("context-usage").unwrap();
            let model = visual.debug_bounds("composer-model-picker").unwrap();
            assert!(intent.right() <= toggle.left());
            assert!(toggle.bottom() <= input.top());
            assert!((toggle.right() - input.right()).abs() <= px(2.));
            assert!(context.right() <= model.left());
            assert!((context.center().y - model.center().y).abs() <= px(4.));
            visual.simulate_click(toggle.center(), Modifiers::default());
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let panel = visual.debug_bounds("composer-task-panel").unwrap();
            assert!(
                panel.size.width <= px(362.),
                "task popover must stay a compact vertical list: {panel:?}"
            );
            let expanded_input = visual.debug_bounds("composer-input-card").unwrap();
            assert_eq!(
                expanded_input, input,
                "task popover must not reflow the input or conversation"
            );
            assert!(
                panel.size.height <= px(402.),
                "task viewport must be bounded: {panel:?}"
            );
            assert!(panel.bottom() <= visual.debug_bounds("composer-status-row").unwrap().top());
            assert!(!view.read_with(&visual, |this, _| this.panel_open));
            let focused = visual.update(|window, cx| window.focused(cx));
            visual.update(|window, cx| view.update(cx, |this, cx| {
                this.on_frame(json!({"type":"tool_execution_update","toolCallId":"task-update","partialResult":{"details":{"phases":[{"name":"实时更新","tasks":[{"content":"任务已更新","status":"completed"}]}]}}}), window, cx);
                cx.notify();
            }));
            visual.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                visual.update(|window, cx| window.focused(cx)),
                focused,
                "live task updates must not steal focus"
            );
            assert!(visual.debug_bounds("composer-task-panel").is_some());
            assert_eq!(visual.debug_bounds("composer-input-card").unwrap(), input);
            assert!(visual.debug_bounds("work-panel-tab-任务").is_none());
            let toggle = visual.debug_bounds("composer-tasks-toggle").unwrap();
            visual.simulate_click(toggle.center(), Modifiers::default());
            visual.update(|window, cx| window.draw(cx).clear(cx));
            assert!(visual.debug_bounds("composer-task-panel").is_none());
        }
    }
    let toggle = visual.debug_bounds("composer-tasks-toggle").unwrap();
    visual.simulate_click(toggle.center(), Modifiers::default());
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.activate(1, window, cx);
            cx.notify();
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("composer-task-panel").is_none());
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.activate(0, window, cx);
            cx.notify();
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("composer-task-panel").is_some());
}

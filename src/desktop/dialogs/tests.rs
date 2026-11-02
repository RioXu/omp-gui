use core::prelude::v1::test;
use gpui_kit::{
    component::{Theme, ThemeMode, WindowExt},
    *,
};
use std::time::Duration;

struct Host;
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}
fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn workspace_dialogs_use_short_motion_and_can_close_during_entrance(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, _) = cx
        .update(|cx| open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Host)).unwrap());
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1000.), px(800.)));
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        visual.update(|window, cx| {
            Theme::change(mode, Some(window), cx);
            cx.set_reduce_motion(false);
            super::open(window, cx, |d, _, _| {
                d.title("确认").child("取消不得更改会话")
            });
        });
        frame(&mut visual);
        let start = visual.debug_bounds("dialog-0").unwrap();
        // Default SlideDown starts at the top; our native FadeSlide stays
        // within one short motion distance of the final position (80px).
        assert!((start.top() - px(80.)).abs() <= px(8.));
        visual
            .background_executor
            .advance_clock(Duration::from_millis(40));
        frame(&mut visual);
        let before = visual.debug_bounds("dialog-0").unwrap();
        visual.update(|window, cx| window.close_dialog(cx));
        frame(&mut visual);
        assert_eq!(
            visual.debug_bounds("dialog-0").unwrap(),
            before,
            "closing midway must not jump"
        );
        visual
            .background_executor
            .advance_clock(Duration::from_millis(250));
        frame(&mut visual);
        frame(&mut visual);
        assert!(visual.debug_bounds("dialog-0").is_none());
    }
    visual.update(|window, cx| {
        cx.set_reduce_motion(true);
        super::open(window, cx, |d, _, _| {
            d.title("减少动态效果").child("立即显示")
        });
    });
    frame(&mut visual);
    assert_eq!(visual.debug_bounds("dialog-0").unwrap().top(), px(80.));
    visual.simulate_keystrokes("escape");
    frame(&mut visual);
    assert!(visual.debug_bounds("dialog-0").is_none());
}

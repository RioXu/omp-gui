use crate::desktop::{
    theme::{self, Skin},
    workspace::runtime::sessions::tests::fixture,
};
use gpui_kit::{
    AppContext, Bounds, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext,
    WindowOptions, component::ActiveTheme, open_window, px, size,
};
use std::time::Duration;
fn frame(cx: &mut VisualTestContext) {
    cx.update(|w, _| w.refresh());
    cx.run_until_parked();
    cx.update(|w, cx| w.draw(cx).clear(cx));
}
fn indicator(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.update(|w, cx| {
        let scale = w.scale_factor();
        let color = gpui_kit::Background::from(cx.theme().primary);
        w.painted_quads()
            .into_iter()
            .map(|q| (q.background, q.bounds.map(|p| px(p.0 / scale))))
            .find(|(c, b)| *c == color && b.size.height == px(2.) && b.left() > px(700.))
            .unwrap()
            .1
    })
}
fn click_tab(cx: &mut VisualTestContext, label: &str) {
    let selector = match label {
        "文件" => "work-panel-tab-文件",
        "变更" => "work-panel-tab-变更",
        _ => "work-panel-tab-子智能体",
    };
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
}
#[gpui_kit::test]
fn panel_underline_reaches_target_promptly_and_handles_rapid_switches(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        theme::apply(Skin::Light, None, cx);
        cx.set_reduce_motion(true);
    });
    let (w, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |w, cx| {
            cx.new(|cx| {
                let mut v = fixture(w, cx);
                v.panel_open = true;
                v
            })
        })
        .unwrap()
    });
    let mut v = VisualTestContext::from_window(w, cx);
    v.simulate_resize(size(px(1240.), px(850.)));
    frame(&mut v);
    frame(&mut v);
    let first = indicator(&mut v);
    v.update(|_, cx| {
        view.update(cx, |s, cx| {
            s.panel_tab = "子智能体";
            cx.notify();
        })
    });
    frame(&mut v);
    frame(&mut v);
    let last = indicator(&mut v);
    assert!(last.left() > first.right());
    v.update(|_, cx| {
        view.update(cx, |s, cx| {
            s.panel_tab = "变更";
            cx.notify();
        })
    });
    frame(&mut v);
    frame(&mut v);
    v.update(|_, cx| cx.set_reduce_motion(false));
    let last_scan = view.read_with(&v, |s, _| s.document_state.last_scan);
    click_tab(&mut v, "子智能体");
    assert_eq!(view.read_with(&v, |s, _| s.panel_tab), "子智能体");
    frame(&mut v);
    v.background_executor
        .advance_clock(Duration::from_millis(16));
    frame(&mut v);
    assert!(indicator(&mut v).left() > first.left() + (last.left() - first.left()) * 0.2);
    for _ in 0..7 {
        v.background_executor
            .advance_clock(Duration::from_millis(16));
        frame(&mut v);
    }
    assert!((indicator(&mut v).left() - last.left()).abs() < px(1.));
    click_tab(&mut v, "文件");
    assert_eq!(view.read_with(&v, |s, _| s.panel_tab), "文件");
    assert_eq!(
        view.read_with(&v, |s, _| s.document_state.last_scan),
        last_scan
    );
    frame(&mut v);
    v.background_executor
        .advance_clock(Duration::from_millis(32));
    frame(&mut v);
    click_tab(&mut v, "变更");
    assert_eq!(view.read_with(&v, |s, _| s.panel_tab), "变更");
    assert_eq!(
        view.read_with(&v, |s, _| s.document_state.last_scan),
        last_scan
    );
    frame(&mut v);
    for _ in 0..8 {
        v.background_executor
            .advance_clock(Duration::from_millis(16));
        frame(&mut v);
    }
    assert!((indicator(&mut v).left() - first.left()).abs() < px(1.));
}

use crate::desktop::workspace::{Workspace, runtime::sessions::tests::fixture};
use crate::domain::Session;
use gpui_kit::{
    AppContext, Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions,
    open_window, point, px, size,
};

#[gpui_kit::test]
fn large_sidebar_draws_visible_sessions_and_collapsing_hides_them(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1100.), px(700.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this: Workspace = fixture(window, cx);
                    this.db.projects.push("fixture".into());
                    for index in 0..2000 {
                        let mut s = Session::new("fixture".into());
                        s.title = format!("session {index}");
                        s.updated = index;
                        this.db.sessions.push(s);
                    }
                    this.db.sessions[2001].id = "visible-fixture".into();
                    this.db.sessions[2].id = "offscreen-fixture".into();
                    this
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        visual
            .debug_bounds("sidebar-session-visible-fixture")
            .is_some()
    );
    assert!(
        visual
            .debug_bounds("sidebar-session-offscreen-fixture")
            .is_none()
    );
    visual.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.collapsed_projects
                .insert(this.db.sessions[0].project_location());
            cx.notify();
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        visual
            .debug_bounds("sidebar-session-visible-fixture")
            .is_none()
    );
}

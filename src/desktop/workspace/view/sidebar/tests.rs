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
                    this.db.sessions[0].id = "first-fixture".into();
                    this.db.sessions[1].id = "second-fixture".into();
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
    let heading = visual.debug_bounds("sidebar-project-row").unwrap();
    let first = visual
        .debug_bounds("sidebar-session-first-fixture")
        .unwrap();
    let second = visual
        .debug_bounds("sidebar-session-second-fixture")
        .unwrap();
    assert!(
        (first.top() - heading.bottom()).abs() <= px(2.),
        "project-to-session gap is too large"
    );
    assert!(
        (second.top() - first.bottom()).abs() <= px(1.),
        "hierarchy guide must stay continuous"
    );
    let guide_painted = visual.update(|window, _| {
        let scale = window.scale_factor();
        window.painted_quads().iter().any(|quad| {
            let bounds = quad.bounds.map(|value| px(value.0 / scale));
            quad.border_widths.left.0 > 0.
                && (bounds.left() - first.left()).abs() <= px(1.)
                && (bounds.top() - first.top()).abs() <= px(1.)
                && bounds.size.height >= first.size.height
        })
    });
    assert!(guide_painted, "session hierarchy line must be painted");
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

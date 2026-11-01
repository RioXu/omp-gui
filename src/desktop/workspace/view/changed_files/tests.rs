use super::{ChangeKind, ChangedFile, changed_file_item};
use gpui_kit::{
    AppContext, Background, Bounds, Context, IntoElement, Modifiers, MouseButton, Render,
    TestAppContext, VisualTestContext, Window, WindowBounds, WindowOptions, component::Theme, div,
    open_window, point, prelude::*, px, rgb, size, uniform_list,
};

struct FileListFixture {
    change: ChangedFile,
    clicked: bool,
}

impl Render for FileListFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(
            uniform_list(
                "changed-files",
                1,
                cx.processor(|this, _, _, cx| {
                    vec![
                        changed_file_item(&this.change, true, false, cx).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.clicked = true;
                                cx.notify();
                            },
                        )),
                    ]
                }),
            )
            .w_full()
            .flex_1()
            .min_h_0(),
        )
    }
}

#[gpui_kit::test]
fn fitting_file_names_and_selected_rows_use_the_available_panel_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for path in ["dev_plan.md", "src/dev_plan.md"] {
        let (window, view) = cx.update(|cx| {
            Theme::update(cx, |t| {
                t.list_active = rgb(0x12abcd).into();
                t.accent = t.list_active;
            });
            let bounds = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(240.)));
            open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|_| FileListFixture {
                        change: ChangedFile {
                            path: path.into(),
                            kind: ChangeKind::Modified,
                        },
                        clicked: false,
                    })
                },
            )
            .unwrap()
        });
        let mut visual = VisualTestContext::from_window(window, cx);
        visual.run_until_parked();
        visual.update(|w, cx| w.draw(cx).clear(cx));
        let name = visual
            .debug_bounds("changed-file-name")
            .expect("filename was drawn");
        assert!(
            name.size.width >= px(200.),
            "filename truncated with unused panel space: {name:?}"
        );
        let highlight = visual.update(|w, _| {
            let scale = w.scale_factor();
            w.painted_quads()
                .iter()
                .find(|q| q.background == Background::from(rgb(0x12abcd)))
                .expect("selected file row was painted")
                .bounds
                .map(|v| px(v.0 / scale))
        });
        assert_eq!(highlight.left(), px(0.));
        assert_eq!(highlight.size.width, px(400.));
        visual.simulate_mouse_down(
            point(px(390.), px(12.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_up(
            point(px(390.), px(12.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            view.read_with(&visual, |view, _| view.clicked),
            "row's right edge must be clickable"
        );
    }
}

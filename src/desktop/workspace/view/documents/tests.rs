use super::file_editor;
use crate::desktop::theme::apply_accent;
use gpui_kit::{
    AppContext, Background, Bounds, Context, Entity, IntoElement, Render, TestAppContext,
    VisualTestContext, Window, WindowBounds, WindowOptions,
    component::{ActiveTheme, Theme, ThemeMode, input::EditorState},
    div, open_window, point,
    prelude::*,
    px, rgb, size,
};

struct FileEditorFixture {
    editor: Entity<EditorState>,
}

impl Render for FileEditorFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background)
            .child(file_editor(&self.editor))
    }
}

#[gpui_kit::test]
fn file_editor_gutter_and_body_paint_the_same_background_in_both_themes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let (window, _) = cx.update(|cx| {
            Theme::change(mode, None, cx);
            Theme::update(cx, |theme| {
                // A syntax theme may bring its own contrasting editor/gutter colors.
                let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
                style.editor_background = Some(rgb(0x765432).into());
                style.editor_gutter_background = Some(rgb(0x123456).into());
            });
            apply_accent(cx);
            let bounds = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(240.)));
            open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| FileEditorFixture {
                        editor: cx.new(|cx| {
                            EditorState::new(window, cx)
                                .line_number(true)
                                .default_value("first line\nsecond line\n")
                        }),
                    })
                },
            )
            .unwrap()
        });
        let mut visual = VisualTestContext::from_window(window, cx);
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        visual.update(|window, cx| {
            let scale = window.scale_factor();
            let gutter = window
                .painted_quads()
                .into_iter()
                .filter(|q| {
                    let bounds = q.bounds.map(|v| px(v.0 / scale));
                    bounds.size.height >= px(200.)
                        && bounds.size.width > px(10.)
                        && bounds.size.width < px(100.)
                })
                .collect::<Vec<_>>();
            assert!(
                !gutter.is_empty(),
                "native editor must paint its line-number gutter"
            );
            assert!(
                gutter
                    .iter()
                    .all(|q| q.background == Background::from(cx.theme().background)),
                "line-number gutter differs from document background in {mode:?}"
            );
        });
    }
}

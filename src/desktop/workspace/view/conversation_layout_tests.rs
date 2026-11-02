use crate::{desktop::workspace::runtime::sessions::tests::fixture, domain::Message};
use gpui_kit::{
    AppContext, Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions,
    component::{Theme, ThemeMode},
    open_window, point, px, size,
};

#[gpui_kit::test]
fn responsive_conversation_and_composer_expand_together_with_ten_percent_side_margins(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1100.), px(850.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.projects.push("fixture".into());
                    this.db.sessions[0].messages.push(Message::new(
                        "assistant",
                        "A short reply for layout verification.".into(),
                    ));
                    this.db.sessions[0]
                        .messages
                        .push(Message::new("custom", "Native notice.".into()));
                    this.runtime.running = true;
                    this.runtime.working_intent = Some(
                        "正在检查项目文件和工具输出，确认宽窗口与窄窗口都保持正确的对齐和摘要展示"
                            .repeat(8),
                    );
                    this.sync_texts(cx);
                    this
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        visual.update(|window, cx| Theme::change(mode, Some(window), cx));
        let mut previous = None;
        for width in [1100., 1500., 1900., 850.] {
            visual.simulate_resize(size(px(width), px(850.)));
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let composer = visual.debug_bounds("conversation-composer-body").unwrap();
            let sidebar_width = view.read_with(&visual, |this, _| this.db.sidebar_width);
            let center_width = width - sidebar_width;
            assert!((composer.size.width - px(center_width * 0.8)).abs() <= px(2.));
            assert!((composer.left() - px(sidebar_width + center_width * 0.1)).abs() <= px(2.));
            let message = visual.debug_bounds("conversation-message-body").unwrap();
            let notice = visual.debug_bounds("conversation-notice-body").unwrap();
            let intent = visual.debug_bounds("working-intent").unwrap();
            let input = visual.debug_bounds("composer-input-card").unwrap();
            assert!((composer.size.width - message.size.width).abs() <= px(2.));
            assert!((composer.size.width - notice.size.width).abs() <= px(2.));
            assert!((composer.left() - message.left()).abs() <= px(2.));
            assert!(intent.bottom() <= input.top(), "intent must be above input");
            assert!(
                intent.size.height <= px(28.),
                "intent must stay on one line"
            );
            if width >= 1500. {
                assert!(message.size.width > px(1000.));
            }
            if let Some((old_width, old_body)) = previous {
                assert!(
                    (composer.size.width - old_body - px((width - old_width) * 0.8)).abs()
                        <= px(2.)
                );
            }
            previous = Some((width, composer.size.width));
        }
    }
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.on_frame(serde_json::json!({"type":"session_settled"}), window, cx);
            cx.notify();
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("working-intent").is_none());
}

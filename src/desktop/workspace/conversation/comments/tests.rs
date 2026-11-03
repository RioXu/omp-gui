use crate::desktop::workspace::{
    conversation::demo::demo_session, runtime::sessions::tests::fixture,
};
use gpui_kit::{
    AppContext, TestAppContext, VisualTestContext, WindowOptions, open_window, px, size,
};

#[gpui_kit::test]
fn locate_original_after_reply_keeps_workspace_alive(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let mut workspace = fixture(window, cx);
                let mut session = demo_session();
                let quote = "需要定位的原文片段";
                let body = format!(
                    "{}{}",
                    "## 原文段落\n\n这里是一段很长的回复，用来复现滚动定位。\n\n".repeat(35),
                    quote
                );
                session.messages[1].text = body.clone();
                session.messages[1]
                    .process
                    .push(crate::domain::ProcessBlock {
                        id: "thinking-fixture".into(),
                        kind: crate::domain::ProcessKind::Thinking,
                        label: "思考".into(),
                        text: "复现定位".into(),
                        streaming: false,
                        result: None,
                        patch: None,
                        computer_operations: Vec::new(),
                    });
                session.comments[0].anchor = crate::domain::Anchor::new(
                    crate::domain::Source::Message(session.messages[1].id.clone()),
                    body.clone(),
                    body.len() - quote.len()..body.len(),
                    quote.into(),
                )
                .unwrap();
                workspace.db.sessions[0] = session;
                workspace.sync_texts(cx);
                workspace
            })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_resize(size(px(1240.), px(850.)));
    visual.run_until_parked();
    visual.update(|w, cx| w.draw(cx).clear(cx));
    for _ in 0..3 {
        visual.update(|_, cx| {
            view.update(cx, |workspace, cx| {
                workspace.runtime.list.scroll_to_end();
                cx.notify();
            })
        });
        visual.run_until_parked();
        visual.update(|_, cx| {
            view.update(cx, |workspace, cx| {
                let anchor = workspace.db.sessions[0].comments[0].anchor.clone();
                workspace.open_anchor(anchor, cx);
            })
        });
        visual.run_until_parked();
        visual.update(|w, cx| w.draw(cx).clear(cx));
        visual.update(|window, cx| {
            use gpui_kit::component::ActiveTheme;
            let workspace = view.read(cx);
            let viewport = workspace.runtime.list.viewport_bounds();
            let scale = window.scale_factor();
            let color = gpui_kit::Background::from(cx.theme().selection);
            assert!(
                window
                    .painted_quads()
                    .iter()
                    .any(|q| q.background == color && {
                        let top = q.bounds.origin.y.0 / scale;
                        let bottom = top + q.bounds.size.height.0 / scale;
                        top >= f32::from(viewport.top()) && bottom <= f32::from(viewport.bottom())
                    }),
                "the exact quote must be highlighted inside the viewport"
            );
            assert!(
                workspace.runtime.list.logical_scroll_top().offset_in_item > px(100.),
                "a long source must reveal the quote, not just the message header"
            );
        });
    }
}

use crate::{
    desktop::workspace::runtime::sessions::tests::fixture,
    domain::{Anchor, Message, Source},
};
use gpui_kit::{
    AppContext, Bounds, ListOffset, Modifiers, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions,
    component::{Theme, ThemeMode},
    open_window, point, px, size,
};

#[gpui_kit::test]
fn notices_preview_three_visual_lines_and_toggle_without_changing_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut message = Message::new(
        "custom",
        format!(
            "<system-notice>\nBackground job TradeSession has completed.\n<task-result id=\"TradeSession\" status=\"failed\">\n<meta lines=\"1\" />\n<output>\n{}TAIL_MARKER\n</output>\n</task-result>\n</system-notice>",
            "Long background result with 中文 text and no explicit line breaks. ".repeat(40)
        ),
    );
    message.label = Some("async-result".into());
    let original = message.text.clone();
    let id = message.id.clone();
    let key = format!("notice-{id}");
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(780.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.sessions[0].messages = vec![message.clone()];
                    this.db.sessions[1].messages = vec![message];
                    this.sync_texts(cx);
                    this.runtime.list.scroll_to(ListOffset {
                        item_ix: 0,
                        offset_in_item: px(0.),
                    });
                    this
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        visual.update(|window, cx| Theme::change(mode, Some(window), cx));
        for width in [1000., 760.] {
            visual.simulate_resize(size(px(width), px(780.)));
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let collapsed = visual.debug_bounds("notice-content").unwrap();
            assert!(
                collapsed.size.height <= px(80.),
                "preview must fit three body lines: {collapsed:?}"
            );
            assert!(view.read_with(&visual, |this, cx| {
                this.runtime.texts[&id].state.read(cx).is_clamped()
            }));
            let button = visual.debug_bounds("notice-toggle").unwrap();
            visual.simulate_click(button.center(), Modifiers::default());
            visual.update(|window, cx| window.draw(cx).clear(cx));
            assert!(view.read_with(&visual, |this, cx| {
                assert!(
                    this.runtime.texts[&id]
                        .state
                        .read(cx)
                        .rendered_text()
                        .as_str()
                        .contains("TAIL_MARKER")
                );
                this.runtime.expanded_process.contains(&key)
            }));
            let expanded = visual.debug_bounds("notice-content").unwrap();
            assert!(
                visual.debug_bounds("notice-toggle").unwrap().top() >= px(0.),
                "expanding a short bottom-aligned history must keep the header visible"
            );
            assert!(expanded.size.height > collapsed.size.height + px(50.));
            let button = visual.debug_bounds("notice-toggle").unwrap();
            visual.simulate_click(button.center(), Modifiers::default());
            visual.update(|window, cx| window.draw(cx).clear(cx));
            assert!(!view.read_with(&visual, |this, _| {
                this.runtime.expanded_process.contains(&key)
            }));
            assert!(visual.debug_bounds("notice-content").unwrap().size.height <= px(80.));
        }
    }
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            assert_eq!(this.db.sessions[0].messages[0].text, original);
            let body = this.runtime.texts[&id].body.clone();
            let range = body.find("TAIL_MARKER").unwrap()..body.find("TAIL_MARKER").unwrap() + 11;
            let anchor = Anchor::new(
                Source::Message(id.clone()),
                body,
                range,
                "TAIL_MARKER".into(),
            )
            .unwrap();
            this.open_anchor(anchor, cx);
            assert!(
                this.runtime.expanded_process.contains(&key),
                "comment reveal must expand its notice"
            );
            this.activate(1, window, cx);
            assert!(
                !this.runtime.expanded_process.contains(&key),
                "same ID in another session must start collapsed"
            );
            this.activate(0, window, cx);
            assert!(this.runtime.expanded_process.contains(&key));
        })
    });
}

#[gpui_kit::test]
fn lsp_notices_render_diagnostics_and_search_expands_hidden_matches(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut message = Message::new(
        "custom",
        "<system-notice>original native text</system-notice>".into(),
    );
    message.label = Some("lsp-late-diagnostic".into());
    let messages = (1..=12).map(|line| format!("src/okx.cpp:{line}:3 [error] [clang] No member in std::vector<A<B>> diagnostic_{line}" )).collect::<Vec<_>>();
    message.details = serde_json::json!({"files":[{"path":"src/okx.cpp","summary":"12 error(s)","messages":messages}]});
    let id = message.id.clone();
    let key = format!("notice-{id}");
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(780.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.sessions[0].messages = vec![message];
                    this.sync_texts(cx);
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
        visual.debug_bounds("notice-content").unwrap().size.height <= px(80.),
        "LSP blocks must retain the three-line collapsed preview"
    );
    view.read_with(&visual, |this, cx| {
        let state = this.runtime.texts[&id].state.read(cx);
        let rendered = state.rendered_text();
        assert!(rendered.as_str().contains("12 个错误"));
        assert!(rendered.as_str().contains("std::vector<A<B>>"));
        assert!(rendered.as_str().contains("错误"));
        assert!(!rendered.as_str().contains("<system-notice>"));
        assert!(state.is_clamped());
    });
    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.search_state
                .input
                .update(cx, |state, cx| state.set_value("diagnostic_12", window, cx));
            this.open_search(window, cx);
            assert_eq!(this.search_state.hits.len(), 1);
            assert!(this.runtime.expanded_process.contains(&key));
            assert_eq!(
                this.db.sessions[0].messages[0].text,
                "<system-notice>original native text</system-notice>"
            );
        })
    });
    visual.update(|window, cx| window.draw(cx).clear(cx));
    assert!(visual.debug_bounds("notice-content").unwrap().size.height > px(80.));
}

#[gpui_kit::test]
fn lsp_long_filenames_wrap_and_severity_markers_fit_narrow_views(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let path = format!(
        "/home/project/long path/{}.py",
        "unbroken_超长文件名".repeat(22)
    );
    let mut message = Message::new("custom", "original Python diagnostics".into());
    message.label = Some("lsp-late-diagnostic".into());
    message.details = serde_json::json!({"files":[{"path":path,"diagnostics":[
        {"severity":1,"source":"Pyright","code":"reportAssignmentType","range":{"start":{"line":0,"character":4}},"message":"Type str is not assignable to int\n  Nested explanation"},
        {"severity":2,"source":"Ruff","code":"F401","range":{"start":{"line":8,"character":0}},"message":"Imported but unused"},
        {"severity":3,"source":"typescript","code":6133,"message":"Information"},
        {"severity":4,"source":"rust-analyzer","message":"Hint"}
    ]}]});
    let id = message.id.clone();
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(1000.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.sessions[0].messages = vec![message];
                    this.sync_texts(cx);
                    this.runtime.expanded_process.insert(format!("notice-{id}"));
                    this.runtime.list.scroll_to(ListOffset {
                        item_ix: 0,
                        offset_in_item: px(0.),
                    });
                    this
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        visual.update(|window, cx| Theme::change(mode, Some(window), cx));
        for width in [1000., 760.] {
            visual.simulate_resize(size(px(width), px(1000.)));
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            let content = visual.debug_bounds("notice-content").unwrap();
            let file = visual.debug_bounds("lsp-file-path").unwrap();
            assert!(
                file.size.height > px(48.),
                "long filename must wrap: {file:?}"
            );
            assert!(file.left() >= content.left() && file.right() <= content.right() + px(1.));
            for marker in [
                "lsp-status-error",
                "lsp-status-warning",
                "lsp-status-information",
                "lsp-status-hint",
            ] {
                let bounds = visual.debug_bounds(marker).unwrap();
                assert!(bounds.left() >= content.left() && bounds.right() <= content.right());
            }
            let text = visual.debug_bounds("lsp-diagnostic-message").unwrap();
            assert!(text.right() <= content.right() + px(1.));
            view.read_with(&visual, |this, cx| {
                let text = this.runtime.texts[&id].state.read(cx).rendered_text();
                assert!(text.as_str().contains(&path));
                assert!(text.as_str().contains("1:5"));
                assert!(!text.as_str().contains('·'));
            });
        }
    }
}

#[gpui_kit::test]
#[ignore = "requires OMP_GUI_PYTHON_LSP_FIXTURE from an actual Pyright publishDiagnostics response"]
fn actual_python_lsp_response_renders_in_native_notice(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let input = std::fs::read(std::env::var("OMP_GUI_PYTHON_LSP_FIXTURE").unwrap()).unwrap();
    let details: serde_json::Value = serde_json::from_slice(&input).unwrap();
    let mut message = Message::new("custom", "native Python response".into());
    message.label = Some("lsp-late-diagnostic".into());
    message.details = details;
    let id = message.id.clone();
    let (window, view) = cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(760.), px(900.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.sessions[0].messages = vec![message];
                    this.sync_texts(cx);
                    this.runtime.expanded_process.insert(format!("notice-{id}"));
                    this.runtime.list.scroll_to(ListOffset {
                        item_ix: 0,
                        offset_in_item: px(0.),
                    });
                    this
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(&visual, |this, cx| {
        let text = this.runtime.texts[&id].state.read(cx).rendered_text();
        for expected in [
            "check.py",
            "2 个错误",
            "Pyright",
            "1:14",
            "2:7",
            "reportAssignmentType",
            "reportUndefinedVariable",
            "Literal['wrong type']",
            "undefined_name",
        ] {
            assert!(
                text.as_str().contains(expected),
                "missing {expected}: {}",
                text.as_str()
            );
        }
        assert!(!text.as_str().contains("file:///"));
        assert!(!text.as_str().contains('·'));
    });
    let content = visual.debug_bounds("notice-content").unwrap();
    let marker = visual.debug_bounds("lsp-status-error").unwrap();
    let body = visual.debug_bounds("lsp-diagnostic-message").unwrap();
    assert!(marker.left() >= content.left());
    assert!(body.right() <= content.right() + px(1.));
}

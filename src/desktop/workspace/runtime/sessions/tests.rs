use crate::{
    desktop::workspace::{DocumentState, PendingSubmission, SearchState, Workspace},
    domain::{Attachment, Database, Message, Session},
    infrastructure::persistence::workspace_store::Persistence,
};
use gpui_kit::{
    AppContext, Bounds, ClipboardItem, Context, ExternalPaths, FileDropEvent, Focusable,
    PlatformInput, Point, TestAppContext, VisualTestContext, Window, WindowBounds, WindowOptions,
    component::{
        input::{InputState, TextareaState},
        tree::TreeState,
    },
    open_window, point, px, size,
};
use serde_json::json;
use std::{collections::HashMap, sync::mpsc, time::Instant};

/// No persisted workspace, native process or model requests in this fixture.
fn fixture(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut db = Database::default();
    for name in ["A", "B"] {
        let mut session = Session::new("fixture".into());
        session.title = name.into();
        session.messages.push(Message::new("user", name.into()));
        db.sessions.push(session);
    }
    db.active = Some(db.sessions[0].id.clone());
    let (tx, rx) = mpsc::channel();
    Workspace {
        runtime: Workspace::new_session_runtime(window, cx, true, "draft A".into(), None),
        background: HashMap::new(),
        polling_background: false,
        attachment_imports: Default::default(),
        environments: Default::default(),
        session_sync: Default::default(),
        selection_dirty: false,
        settings_view: None,
        show_archived: false,
        collapsed_projects: Default::default(),
        db,
        active: Some(0),
        persistence: Persistence::disconnected(),
        save_disabled: true,
        comment_input: cx.new(|cx| TextareaState::new(window, cx)),
        focus: cx.focus_handle(),
        panel_open: false,
        panel_tab: "变更",
        selected: None,
        selection_point: Point::default(),
        comment_editor: None,
        editing_comment: None,
        pending_open: false,
        demo: true,
        last_save: Instant::now(),
        dirty: false,
        _subscriptions: vec![],
        _data_lock: None,
        search_state: SearchState {
            input: cx.new(|cx| InputState::new(window, cx)),
            open: false,
            scope: None,
            source: String::new(),
            diff_positions: vec![],
            hits: vec![],
            index: 0,
            rendered_text: HashMap::new(),
        },
        document_state: DocumentState {
            tabs: vec![],
            subscriptions: Default::default(),
            prompt: None,
            discard_permit: false,
            active: None,
            watcher: None,
            watched_session: String::new(),
            watch_retry: None,
            refresh_pending: None,
            last_scan: Instant::now(),
            tree: cx.new(|cx| TreeState::new(cx)),
            diff: None,
            quote: None,
            text: None,
            files: vec![],
            file_link_revision: 0,
            references: Default::default(),
            changes: vec![],
            tx,
            rx,
            snapshot: None,
            path: None,
            historical: false,
            loading: false,
        },
    }
}

#[gpui_kit::test]
fn native_file_drop_reaches_composer_and_plain_text_paste_remains_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
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
                    let mut workspace = fixture(window, cx);
                    workspace.panel_open = false;
                    workspace.save_disabled = false;
                    workspace
                })
            },
        )
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let position = point(px(500.), px(595.));
    visual.update(|window, cx| {
        let missing =
            std::env::temp_dir().join(format!("omp-gui-missing-{}", uuid::Uuid::new_v4()));
        let paths = ExternalPaths(vec![missing].into());
        window.dispatch_event(
            PlatformInput::FileDrop(FileDropEvent::Entered { position, paths }),
            cx,
        );
        window.draw(cx).clear(cx);
        window.dispatch_event(
            PlatformInput::FileDrop(FileDropEvent::Submit { position }),
            cx,
        );
        view.update(cx, |this, cx| {
            this.save_disabled = true;
            assert!(
                this.importing_attachments(),
                "native external drop must reach the composer"
            );
            assert!(!this.paste_attachments(&ClipboardItem::new_string("plain text".into()), cx));
            this.runtime
                .composer_state
                .input
                .focus_handle(cx)
                .focus(window, cx);
        });
    });
}

#[gpui_kit::test]
fn concurrent_streams_and_background_submission_recovery_stay_with_their_owner(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle
        .update(cx, |_, window, cx| window_view_update(&view, window, cx))
        .unwrap();
}

fn window_view_update(
    view: &gpui_kit::Entity<Workspace>,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    view.update(cx, |this, cx| {
        let a = this.db.sessions[0].id.clone();
        let b = this.db.sessions[1].id.clone();
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        this.runtime.context_used = Some(42);
        this.activate(1, window, cx);
        this.runtime.composer_state.input.update(cx, |input, cx| input.set_value("draft B", window, cx));
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        assert!(this.session_is_running(&a) && this.session_is_running(&b));
        this.dispatch_background(&a, window, cx, |this, window, cx| {
            this.on_frame(json!({"type":"message_update","messageId":"same-id","message":{"role":"assistant","content":[{"type":"text","text":"answer A"}]}}), window, cx);
        });
        this.on_frame(json!({"type":"message_update","messageId":"same-id","message":{"role":"assistant","content":[{"type":"text","text":"answer B"}]}}), window, cx);
        assert_eq!(this.db.sessions[0].messages.last().unwrap().text, "answer A");
        assert_eq!(this.db.sessions[1].messages.last().unwrap().text, "answer B");
        let attachment = Attachment { id: "image".into(), name: "image.png".into(), path: "cache.png".into(), original_path: None, mime: Some("image/png".into()), bytes: 5 };
        this.dispatch_background(&a, window, cx, |this, window, cx| {
            this.runtime.pending_send = Some(PendingSubmission { comment_ids:vec![],id: "pending-a".into(), text: "restore A".into(), follow_up: None, was_running: true, wire_text: "wire A".into(), delivered: false, attachments: vec![attachment] });
            this.on_frame(json!({"type":"response","command":"prompt","id":"pending-a","success":false,"error":"fixture failure"}), window, cx);
            this.on_frame(json!({"type":"session_settled"}), window, cx);
        });
        assert!(!this.session_is_running(&a));
        assert!(this.session_is_running(&b));
        assert_eq!(this.runtime.composer_state.input.read(cx).value().as_ref(), "draft B");
        assert!(this.db.sessions[1].draft_attachments.is_empty());
        assert!(this.db.sessions[0].draft.contains("restore A"));
        assert_eq!(this.db.sessions[0].draft_attachments.len(), 1);
        assert_eq!(this.db.active.as_ref(), Some(&b));
        this.activate(0, window, cx);
        assert_eq!(this.runtime.context_used, Some(42));
        assert_eq!(this.runtime.error.as_deref(), Some("fixture failure"));
        assert!(this.runtime.composer_state.input.read(cx).value().contains("restore A"));
        assert!(this.session_is_running(&b));
    });
}

#[gpui_kit::test]
fn asynchronous_save_acknowledgements_preserve_newer_edits_and_ignore_stale_requests(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |this, cx| {
                this.open_document("a.rs".into(), false);
                let key = this.document_state.tabs[0].key.clone();
                let editor = cx.new(|cx| gpui_kit::component::input::EditorState::new(window, cx));
                editor.update(cx, |editor, cx| {
                    editor.set_value("newer unsaved edit", window, cx)
                });
                let tab = &mut this.document_state.tabs[0];
                tab.editor = Some(editor);
                tab.buffer = Some(crate::domain::file_buffer::FileBuffer::new("original"));
                tab.dirty = true;
                tab.saving = Some("current".into());
                this.finish_save(
                    &key,
                    "stale",
                    ("stale".into(), "stale".into()),
                    Ok(()),
                    window,
                    cx,
                );
                assert_eq!(
                    this.document_state.tabs[0]
                        .buffer
                        .as_ref()
                        .unwrap()
                        .disk_text(),
                    "original"
                );
                this.finish_save(
                    &key,
                    "current",
                    ("saved revision".into(), "saved revision".into()),
                    Ok(()),
                    window,
                    cx,
                );
                let tab = &this.document_state.tabs[0];
                assert!(tab.is_dirty(cx));
                assert_eq!(tab.buffer.as_ref().unwrap().disk_text(), "saved revision");
                this.document_state.tabs[0].saving = Some("failed".into());
                this.finish_save(
                    &key,
                    "failed",
                    ("never saved".into(), "never saved".into()),
                    Err("connection lost".into()),
                    window,
                    cx,
                );
                let tab = &this.document_state.tabs[0];
                assert!(tab.is_dirty(cx));
                assert_eq!(tab.buffer.as_ref().unwrap().disk_text(), "saved revision");
                assert_eq!(tab.error.as_deref(), Some("connection lost"));
            })
        })
        .unwrap();
}

#[gpui_kit::test]
fn remote_interactive_request_replay_is_deduplicated_and_cancelled_requests_are_removed(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle.update(cx,|_,window,cx|view.update(cx,|this,cx|{
        let request=json!({"type":"extension_ui_request","id":"ui-1","method":"input","title":"fixture question"});
        this.on_frame(request.clone(),window,cx);this.on_frame(request,window,cx);
        assert_eq!(this.runtime.requests.len(),1);
        this.on_frame(json!({"type":"__omp_gui_transport","state":"connected","requests":["ui-1"]}),window,cx);
        assert_eq!(this.runtime.requests.len(),1);
        this.on_frame(json!({"type":"__omp_gui_transport","state":"connected","requests":[]}),window,cx);
        assert!(this.runtime.requests.is_empty());
    })).unwrap();
}

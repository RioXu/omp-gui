use super::tests::fixture;
use super::*;
use core::prelude::v1::test;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
fn working_intents_follow_their_session_and_clear_on_settle_or_new_run(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, view) = cx.update(|cx| {
        open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| fixture(window, cx))
        })
        .unwrap()
    });
    handle.update(cx, |_, window, cx| view.update(cx, |this, cx| {
        let a = this.db.sessions[0].id.clone();
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        this.on_frame(json!({"type":"tool_execution_start","toolCallId":"a1","toolName":"read","args":{},"intent":"检查文件 A..."}), window, cx);
        assert_eq!(this.runtime.working_intent.as_deref(), Some("检查文件 A"));
        this.activate(1, window, cx);
        assert!(this.runtime.working_intent.is_none());
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        this.on_frame(json!({"type":"tool_execution_start","toolCallId":"b1","toolName":"read","args":{},"intent":"检查文件 B"}), window, cx);
        this.dispatch_background(&a, window, cx, |this, window, cx| {
            this.on_frame(json!({"type":"tool_execution_start","toolCallId":"a2","toolName":"read","args":{},"intent":"检查文件 A2"}), window, cx);
        });
        assert_eq!(this.runtime.working_intent.as_deref(), Some("检查文件 B"));
        assert_eq!(this.background[&a].working_intent.as_deref(), Some("检查文件 A2"));
        this.runtime.stopping = true;
        this.on_frame(json!({"type":"tool_execution_start","toolCallId":"late","toolName":"read","args":{},"intent":"late"}), window, cx);
        assert_eq!(this.runtime.working_intent.as_deref(), Some("检查文件 B"));
        this.on_frame(json!({"type":"prompt_result","sessionSettled":false}), window, cx);
        assert!(this.runtime.working_intent.is_some());
        this.on_frame(json!({"type":"session_settled","status":"aborted"}), window, cx);
        assert!(this.runtime.working_intent.is_none());
        this.activate(0, window, cx);
        assert_eq!(this.runtime.working_intent.as_deref(), Some("检查文件 A2"));
        this.runtime.working_intent = Some("previous run".into());
        this.on_frame(json!({"type":"agent_start"}), window, cx);
        assert!(this.runtime.working_intent.is_none());
    })).unwrap();
}

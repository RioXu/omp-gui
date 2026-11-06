use super::*;

impl Workspace {
    pub fn cancel_queued(&mut self, text: String, cx: &mut Context<Self>) {
        if !self.runtime.connected
            || self.runtime.stopping
            || self.runtime.pending_send.is_some()
            || self.runtime.steering_request.is_some()
            || self.runtime.cancel_queued_request.is_some()
            || !self.runtime.queued_inputs.contains(&text)
        {
            return;
        }
        let request = id();
        let frame =
            json!({"type":"remove_queued_message","id":request,"message":text,"queue":"followUp"});
        let result = self
            .runtime
            .engine
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OMP 连接不可用"))
            .and_then(|engine| engine.send(frame));
        if let Err(error) = result {
            self.runtime.error = Some(format!("取消追加输入失败：{error}"));
        } else {
            self.runtime.cancel_queued_request = Some((request, text));
            self.runtime.status = "正在取消追加输入…".into();
        }
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn queued_removed(
        &mut self,
        frame: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((request, wire)) = self.runtime.cancel_queued_request.take() else {
            return;
        };
        if frame["id"] != request {
            self.runtime.cancel_queued_request = Some((request, wire));
            return;
        }
        if frame["success"] == false {
            self.runtime.error = Some(frame["error"].as_str().unwrap_or("取消追加输入失败").into());
            return;
        }
        if frame["data"]["removed"] != true {
            self.runtime.status = "该输入已被接收或已离开等待队列".into();
            self.request_state();
            return;
        }
        let entry = self
            .runtime
            .submitted_inputs
            .iter()
            .position(|e| {
                e.wire_text == wire
                    && self.session().is_some_and(|s| {
                        s.messages
                            .iter()
                            .any(|m| m.id == e.message_id && m.state == MessageState::Queued)
                    })
            })
            .map(|ix| self.runtime.submitted_inputs.remove(ix));
        let text = entry
            .as_ref()
            .map_or(wire.as_str(), |e| e.text.as_str())
            .to_owned();
        if let Some(entry) = entry {
            if let Some(session) = self.session_mut() {
                if let Some(message) = session
                    .messages
                    .iter_mut()
                    .find(|m| m.id == entry.message_id)
                {
                    message.state = MessageState::Cancelled;
                }
                crate::domain::attachments::merge_attachments(
                    &mut session.draft_attachments,
                    entry.attachments,
                );
                for comment in &mut session.comments {
                    if entry.comment_ids.contains(&comment.id) {
                        comment.sent = false;
                    }
                }
            }
            self.runtime
                .in_flight_comments
                .retain(|id| !entry.comment_ids.contains(id));
        }
        if let Some(ix) = self
            .runtime
            .queued_inputs
            .iter()
            .position(|item| item == &wire)
        {
            self.runtime.queued_inputs.remove(ix);
        }
        let draft = self.runtime.composer_state.input.read(cx).value();
        let restored = if draft.is_empty() {
            text
        } else {
            format!("{text}\n\n{draft}")
        };
        self.runtime.composer_state.input.update(cx, |input, cx| {
            input.set_value(restored.clone(), window, cx)
        });
        if let Some(session) = self.session_mut() {
            session.draft = restored;
        }
        self.runtime.status = "已取消追加输入并恢复到输入框".into();
        self.sync_texts(cx);
        self.persist();
        self.request_state();
    }

    pub fn steer_queued(&mut self, text: String, cx: &mut Context<Self>) {
        if self.runtime.stopping
            || self.runtime.cancel_queued_request.is_some()
            || self.runtime.pending_send.is_some()
            || self.runtime.steering_request.is_some()
        {
            return;
        }
        self.runtime.steering_request = Some(text.clone());
        self.command(json!({"type":"promote_queued_message","id":"steer-queued","message":text}));
        self.runtime.status = "正在向 OMP 提交指示…".into();
        cx.notify();
    }
    pub(in crate::desktop::workspace) fn queue_delivered(&mut self, text: &str) {
        if let Some(pending) = &mut self.runtime.pending_send
            && pending.wire_text.trim() == text.trim()
        {
            pending.delivered = true;
        }
        let entry = self
            .runtime
            .submitted_inputs
            .iter()
            .find(|e| {
                e.wire_text.trim() == text.trim()
                    && self.session().is_some_and(|s| {
                        s.messages
                            .iter()
                            .any(|m| m.id == e.message_id && m.state == MessageState::Queued)
                    })
            })
            .map(|e| (e.message_id.clone(), e.comment_ids.clone()));
        if let Some((mid, comments)) = entry {
            if let Some(s) = self.session_mut()
                && let Some(ix) = s.messages.iter().position(|m| m.id == mid)
            {
                let mut message = s.messages.remove(ix);
                message.state = MessageState::Normal;
                s.messages.push(message);
            }
            self.runtime.in_flight_comments.extend(comments);
            self.runtime.text_update_pending = true;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::workspace::runtime::sessions::tests::fixture;
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;
    use std::sync::{Arc, Mutex};
    struct Capture(Arc<Mutex<Vec<Value>>>);
    impl crate::infrastructure::omp::SessionConnection for Capture {
        fn send(&self, frame: Value) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(frame);
            Ok(())
        }
        fn poll(&self, _: usize) -> Vec<crate::infrastructure::omp::Event> {
            vec![]
        }
    }
    #[gpui_kit::test]
    fn cancellation_waits_for_native_ack_and_restores_one_input_without_stopping(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| fixture(window, cx))
            })
            .unwrap()
        });
        window.update(cx, |_, window, cx| view.update(cx, |this, cx| {
            let frames = Arc::new(Mutex::new(vec![]));
            this.runtime.engine = Some(Box::new(Capture(frames.clone())));
            this.runtime.connected = true;
            this.runtime.running = true;
            let mut message = Message::new("user", "取消这一条".into());
            message.state = MessageState::Queued;
            let mid = message.id.clone();
            let attachment = Attachment { id:"a".into(),name:"image.png".into(),path:"cache.png".into(),original_path:None,mime:Some("image/png".into()),bytes:5 };
            this.session_mut().unwrap().messages = vec![message];
            this.runtime.submitted_inputs.push(SubmittedInput { message_id:mid, wire_text:"wire".into(),text:"取消这一条".into(),comment_ids:vec![],attachments:vec![attachment] });
            this.runtime.queued_inputs = vec!["wire".into(), "keep".into()];
            this.cancel_queued("wire".into(), cx);
            let request = this.runtime.cancel_queued_request.as_ref().unwrap().0.clone();
            assert_eq!(frames.lock().unwrap()[0]["queue"], "followUp");
            assert_eq!(this.runtime.queued_inputs.len(), 2);
            assert_eq!(this.session().unwrap().messages[0].state, MessageState::Queued);
            this.on_frame(json!({"type":"response","command":"remove_queued_message","id":request,"success":true,"data":{"removed":true}}), window, cx);
            assert_eq!(this.runtime.queued_inputs, vec!["keep"]);
            assert!(this.runtime.running);
            assert_eq!(this.session().unwrap().messages[0].state, MessageState::Cancelled);
            assert_eq!(this.session().unwrap().draft_attachments.len(), 1);
            assert_eq!(this.runtime.composer_state.input.read(cx).value().as_str(), "取消这一条\n\ndraft A");
            assert!(!frames.lock().unwrap().iter().any(|f| f["type"] == "abort"));
            this.runtime.queued_inputs.push("late".into());
            this.cancel_queued("late".into(), cx);
            let request = this.runtime.cancel_queued_request.as_ref().unwrap().0.clone();
            let draft = this.runtime.composer_state.input.read(cx).value();
            this.on_frame(json!({"type":"response","command":"remove_queued_message","id":request,"success":true,"data":{"removed":false}}), window, cx);
            assert_eq!(this.runtime.composer_state.input.read(cx).value(), draft);
            this.runtime.steered_inputs.push("steered".into());
            this.cancel_queued("steered".into(), cx);
            assert!(this.runtime.cancel_queued_request.is_none());
        })).unwrap();
    }
}

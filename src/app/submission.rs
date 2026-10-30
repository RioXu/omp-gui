use super::*;

pub(crate) struct PendingSubmission {
    pub id: String,
    pub text: String,
    pub follow_up: Option<String>,
    pub was_running: bool,
    pub wire_text: String,
}
pub(crate) struct SubmittedInput {
    pub message_id: String,
    pub wire_text: String,
    pub text: String,
    pub comment_ids: Vec<String>,
}
impl Workspace {
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.stopping {
            return;
        }
        self.stopping = true;
        self.interrupted_run = true;
        self.command(json!({"type":"abort_and_restore_queue","id":id()}));
        self.status = "正在停止…".into();
        cx.notify();
    }
    pub(super) fn accept_submission(
        &mut self,
        frame: &Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.pending_send.take() else {
            return;
        };
        if frame["id"] != pending.id {
            self.pending_send = Some(pending);
            return;
        }
        if frame["success"] == false {
            self.running = pending.was_running;
            let current = self.input.read(cx).value().to_string();
            let restored = if current.is_empty() {
                pending.text
            } else {
                format!("{}\n\n{current}", pending.text)
            };
            self.input
                .update(cx, |input, cx| input.set_value(restored, w, cx));
            self.follow_up = pending.follow_up;
        } else {
            if let Some(s) = self.session_mut() {
                if s.title == "新建会话" {
                    s.title = pending.text.chars().take(28).collect();
                    if s.title.trim().is_empty() {
                        s.title = "评论跟进".into();
                    }
                }
                let (message_id, ids) = s.submit(pending.text.clone(), pending.follow_up);
                self.submitted_inputs.push(SubmittedInput {
                    message_id,
                    wire_text: pending.wire_text,
                    text: pending.text,
                    comment_ids: ids.clone(),
                });
                self.in_flight_comments.extend(ids);
                self.in_flight_comments.sort();
                self.in_flight_comments.dedup();
            }
            // A local slash command does not mean an already-running session settled.
            if frame["data"]["agentInvoked"] == false && !pending.was_running {
                self.running = false;
            }
        }
        self.text_update_pending = true;
        self.persist();
        self.request_state();
    }
    pub(super) fn restore_queue(&mut self, frame: &Value, w: &mut Window, cx: &mut Context<Self>) {
        self.stopping = false;
        let withdrawn = ["steering", "followUp"]
            .into_iter()
            .flat_map(|key| frame["data"][key].as_array().into_iter().flatten())
            .filter_map(|v| v["text"].as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut restored = Vec::new();
        for wire in withdrawn {
            if let Some(ix) = self
                .submitted_inputs
                .iter()
                .rposition(|s| s.wire_text == wire)
            {
                let entry = self.submitted_inputs.remove(ix);
                if let Some(s) = self.session_mut() {
                    if let Some(m) = s.messages.iter_mut().find(|m| m.id == entry.message_id) {
                        m.state = MessageState::Cancelled;
                    }
                    for c in &mut s.comments {
                        if entry.comment_ids.contains(&c.id) {
                            c.sent = false;
                        }
                    }
                }
                self.in_flight_comments
                    .retain(|id| !entry.comment_ids.contains(id));
                restored.push(entry.text);
            } else {
                restored.push(wire);
            }
        }
        let restored = restored.join("\n\n");
        if !restored.is_empty() {
            let draft = self.input.read(cx).value().to_string();
            self.input.update(cx, |input, cx| {
                input.set_value(
                    if draft.is_empty() {
                        restored
                    } else {
                        format!("{restored}\n\n{draft}")
                    },
                    w,
                    cx,
                )
            });
            self.status = "已停止；未执行的追加指令已恢复到输入框".into();
        } else {
            self.status = "已停止".into();
        }
        self.queued_inputs.clear();
        self.running = false;
        self.persist();
        self.refresh_transcript();
        self.request_state();
    }
}

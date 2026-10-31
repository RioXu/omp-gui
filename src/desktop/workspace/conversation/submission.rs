use super::*;

pub(crate) struct PendingSubmission {
    pub id: String,
    pub text: String,
    pub follow_up: Option<String>,
    pub was_running: bool,
    pub wire_text: String,
    pub delivered: bool,
    pub attachments: Vec<Attachment>,
}
pub(crate) struct SubmittedInput {
    pub message_id: String,
    pub wire_text: String,
    pub text: String,
    pub comment_ids: Vec<String>,
    pub attachments: Vec<Attachment>,
}
impl Workspace {
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.runtime.running || self.runtime.stopping {
            return;
        }
        self.runtime.stopping = true;
        self.runtime.interrupted_run = true;
        self.command(json!({"type":"abort_and_restore_queue","id":id()}));
        self.runtime.status = "正在停止…".into();
        cx.notify();
    }
    pub(in crate::desktop::workspace) fn accept_submission(
        &mut self,
        frame: &Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.runtime.pending_send.take() else {
            return;
        };
        if frame["id"] != pending.id {
            self.runtime.pending_send = Some(pending);
            return;
        }
        if frame["success"] == false {
            self.runtime.running = pending.was_running;
            let current = self
                .runtime
                .composer_state
                .input
                .read(cx)
                .value()
                .to_string();
            let restored = if current.is_empty() {
                pending.text
            } else {
                format!("{}\n\n{current}", pending.text)
            };
            self.runtime
                .composer_state
                .input
                .update(cx, |input, cx| input.set_value(restored.clone(), w, cx));
            if let Some(s) = self.session_mut() {
                s.draft = restored;
                crate::domain::attachments::merge_attachments(
                    &mut s.draft_attachments,
                    pending.attachments,
                );
            }
            self.runtime.follow_up = pending.follow_up;
        } else {
            let mut new_title = None;
            if let Some(s) = self.session_mut() {
                if s.title == "新建会话" {
                    s.title = pending.text.chars().take(28).collect();
                    if s.title.trim().is_empty() {
                        s.title = pending
                            .attachments
                            .first()
                            .map(|a| a.name.clone())
                            .unwrap_or_else(|| "评论跟进".into());
                    }
                    new_title = Some(s.title.clone());
                }
                let (message_id, ids) = s.submit(pending.text.clone(), pending.follow_up);
                if let Some(m) = s.messages.last_mut() {
                    m.native_text = Some(pending.wire_text.clone());
                    m.attachments = pending.attachments.clone();
                }
                let queued = pending.was_running
                    && !pending.delivered
                    && frame["data"]["agentInvoked"] != false;
                if queued && let Some(m) = s.messages.last_mut() {
                    m.state = MessageState::Queued;
                }
                self.runtime.submitted_inputs.push(SubmittedInput {
                    message_id,
                    wire_text: pending.wire_text,
                    text: pending.text,
                    comment_ids: ids.clone(),
                    attachments: pending.attachments,
                });
                if !queued {
                    self.runtime.in_flight_comments.extend(ids);
                }
                self.runtime.in_flight_comments.sort();
                self.runtime.in_flight_comments.dedup();
            }
            if let Some(name) = new_title {
                self.command(json!({"type":"set_session_name","id":id(),"name":name}));
            }
            // A local slash command does not mean an already-running session settled.
            if frame["data"]["agentInvoked"] == false && !pending.was_running {
                self.runtime.running = false;
            }
        }
        self.runtime.text_update_pending = true;
        self.persist();
        self.request_state();
    }
    pub(in crate::desktop::workspace) fn restore_queue(
        &mut self,
        frame: &Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime.stopping = false;
        let withdrawn = ["steering", "followUp"]
            .into_iter()
            .flat_map(|key| frame["data"][key].as_array().into_iter().flatten())
            .filter_map(|v| v["text"].as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut restored = Vec::new();
        for wire in withdrawn {
            if let Some(ix) = self
                .runtime
                .submitted_inputs
                .iter()
                .rposition(|s| s.wire_text == wire)
            {
                let entry = self.runtime.submitted_inputs.remove(ix);
                if let Some(s) = self.session_mut() {
                    crate::domain::attachments::merge_attachments(
                        &mut s.draft_attachments,
                        entry.attachments,
                    );
                    if let Some(m) = s.messages.iter_mut().find(|m| m.id == entry.message_id) {
                        m.state = MessageState::Cancelled;
                    }
                    for c in &mut s.comments {
                        if entry.comment_ids.contains(&c.id) {
                            c.sent = false;
                        }
                    }
                }
                self.runtime
                    .in_flight_comments
                    .retain(|id| !entry.comment_ids.contains(id));
                restored.push(entry.text);
            } else {
                restored.push(wire);
            }
        }
        let restored = restored.join("\n\n");
        if !restored.is_empty() {
            let draft = self
                .runtime
                .composer_state
                .input
                .read(cx)
                .value()
                .to_string();
            self.runtime.composer_state.input.update(cx, |input, cx| {
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
            self.runtime.status = "已停止；未执行的追加指令已恢复到输入框".into();
        } else {
            self.runtime.status = "已停止".into();
        }
        let draft = self
            .runtime
            .composer_state
            .input
            .read(cx)
            .value()
            .to_string();
        if let Some(s) = self.session_mut() {
            s.draft = draft;
        }
        self.runtime.queued_inputs.clear();
        self.runtime.steered_inputs.clear();
        self.runtime.running = false;
        self.runtime.text_update_pending = true;
        self.persist();
        self.refresh_transcript();
        self.request_state();
    }
}

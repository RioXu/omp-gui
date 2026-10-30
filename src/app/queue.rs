use super::*;

impl Workspace {
    pub fn steer_queued(&mut self, text: String, cx: &mut Context<Self>) {
        if self.stopping || self.pending_send.is_some() || self.steering_request.is_some() {
            return;
        }
        self.steering_request = Some(text.clone());
        self.command(json!({"type":"promote_queued_message","id":"steer-queued","message":text}));
        self.status = "正在向 OMP 提交指示…".into();
        cx.notify();
    }
    pub(super) fn queue_delivered(&mut self, text: &str) {
        if let Some(pending) = &mut self.pending_send
            && pending.wire_text.trim() == text.trim()
        {
            pending.delivered = true;
        }
        let entry = self
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
            self.in_flight_comments.extend(comments);
            self.text_update_pending = true;
            self.dirty = true;
        }
    }
}

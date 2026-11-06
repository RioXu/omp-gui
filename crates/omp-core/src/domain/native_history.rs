//! Reconcile native history while retaining GUI comment anchors and drafts.
use super::{Message, MessageState, Session};
impl Session {
    pub fn merge_native_history(&mut self, native: Vec<Message>) {
        let mut previous: Vec<Option<Message>> = std::mem::take(&mut self.messages)
            .into_iter()
            .map(Some)
            .collect();
        let mut cursor = 0;
        let mut anchor = None;
        let mut replies = Vec::new();
        for message in previous.iter().flatten() {
            if message.is_interaction_reply() {
                replies.push((message.clone(), anchor.clone()));
            }
            anchor = Some(message.id.clone());
        }
        self.messages = native
            .into_iter()
            .map(|mut incoming| {
                let exact = previous.iter().position(|m| {
                    m.as_ref().is_some_and(|m| {
                        m.id == incoming.id
                            || (m.native_id.is_some() && m.native_id == incoming.native_id)
                    })
                });
                let matched = exact.or_else(|| {
                    previous
                        .iter()
                        .enumerate()
                        .skip(cursor)
                        .find_map(|(ix, m)| {
                            let m = m.as_ref()?;
                            if m.is_interaction_reply() {
                                return None;
                            }
                            let same_text =
                                m.native_text.as_ref().unwrap_or(&m.text) == &incoming.text;
                            // Older GUI versions did not retain the augmented wire prompt.
                            let legacy_comment = !m.comments.is_empty()
                                && incoming.role == "user"
                                && incoming.text.starts_with(&format!(
                                    "{}\n\n以下是用户对既有内容的反馈。",
                                    m.text
                                ));
                            (m.native_id.is_none()
                                && m.role == incoming.role
                                && (same_text || legacy_comment))
                                .then_some(ix)
                        })
                });
                if let Some(ix) = matched {
                    cursor = ix + 1;
                    let old = previous[ix].take().unwrap();
                    incoming.id = old.id;
                    incoming.comments = old.comments;
                    if !old.attachments.is_empty() {
                        incoming.attachments = old.attachments;
                    }
                    if incoming.role == "user" {
                        incoming.native_text = Some(incoming.text);
                        incoming.text = old.text;
                    }
                    for block in &mut incoming.process {
                        if let Some(prior) = old.process.iter().find(|b| b.id == block.id) {
                            if block.patch.is_none() {
                                block.patch = prior.patch.clone();
                            }
                            if block.result.is_none() {
                                block.result = prior.result.clone();
                            }
                        }
                    }
                }
                incoming
            })
            .collect();
        // Native logs store extension answers in tool results, not user turns.
        // Keep their visible GUI replies next to the original question/answer.
        for (reply, anchor) in replies {
            let at = anchor
                .and_then(|id| self.messages.iter().position(|m| m.id == id))
                .map_or(self.messages.len(), |ix| ix + 1);
            self.messages.insert(at, reply);
        }
        // Unsent/withdrawn inputs are GUI records, not durable native turns.
        self.messages
            .extend(previous.into_iter().flatten().filter(|m| {
                m.state != MessageState::Normal
                    && m.native_id.is_none()
                    && !m.is_interaction_reply()
            }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn interaction_replies_keep_order_and_do_not_consume_equal_native_prompts() {
        let question = Message::new("assistant", "question".into());
        let mut first = Message::new("user", "same text".into());
        first.details = json!({"omp_gui_interaction":{"request_id":"first"}});
        let mut second = Message::new("user", "second answer".into());
        second.details = json!({"omp_gui_interaction":{"request_id":"second"}});
        let mut session = Session::new("fixture".into());
        session.messages = vec![question.clone(), first.clone(), second.clone()];
        let native_user = Message::new("user", "same text".into());
        for _ in 0..2 {
            session.merge_native_history(vec![question.clone(), native_user.clone()]);
            assert_eq!(session.messages.len(), 4);
            assert_eq!(session.messages[1].id, first.id);
            assert_eq!(session.messages[2].id, second.id);
            assert_eq!(session.messages[3].id, native_user.id);
            assert!(!session.messages[3].is_interaction_reply());
        }
    }
}

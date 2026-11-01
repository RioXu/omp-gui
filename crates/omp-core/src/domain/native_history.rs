//! Reconcile native history while retaining GUI comment anchors and drafts.
use super::{Message, MessageState, Session};
impl Session {
    pub fn merge_native_history(&mut self, native: Vec<Message>) {
        let mut previous: Vec<Option<Message>> = std::mem::take(&mut self.messages)
            .into_iter()
            .map(Some)
            .collect();
        let mut cursor = 0;
        self.messages = native
            .into_iter()
            .map(|mut incoming| {
                let exact = previous.iter().position(|m| {
                    m.as_ref()
                        .is_some_and(|m| m.native_id.is_some() && m.native_id == incoming.native_id)
                });
                let matched = exact.or_else(|| {
                    previous
                        .iter()
                        .enumerate()
                        .skip(cursor)
                        .find_map(|(ix, m)| {
                            let m = m.as_ref()?;
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
        // Unsent/withdrawn inputs are GUI records, not durable native turns.
        self.messages.extend(
            previous
                .into_iter()
                .flatten()
                .filter(|m| m.state != MessageState::Normal && m.native_id.is_none()),
        );
    }
}

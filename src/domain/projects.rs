use super::{Database, Session};
impl Session {
    pub fn recover_unconfirmed_queue(&mut self) {
        let mut drafts = Vec::new();
        for m in &mut self.messages {
            if m.state == super::MessageState::Queued {
                m.state = super::MessageState::Unconfirmed;
                drafts.push(m.text.clone());
                super::attachments::merge_attachments(
                    &mut self.draft_attachments,
                    m.attachments.clone(),
                );
            }
        }
        if !drafts.is_empty() {
            if !self.draft.is_empty() {
                drafts.push(self.draft.clone());
            }
            self.draft = drafts.join("\n\n");
        }
    }
    pub fn is_blank(&self) -> bool {
        self.omp_session.is_none() && self.is_untouched()
    }
    pub fn is_untouched(&self) -> bool {
        self.title == "新建会话"
            && !self.archived
            && !self.deleted
            && self.messages.is_empty()
            && self.comments.is_empty()
            && self.draft.trim().is_empty()
            && self.draft_attachments.is_empty()
            && self.snapshots.is_empty()
            && self.todos.is_empty()
            && self.agents.is_empty()
    }
}
impl Database {
    pub fn discard_blank_sessions(&mut self, keep: Option<&str>) {
        self.sessions
            .retain(|s| Some(s.id.as_str()) == keep || !s.is_blank());
        if self
            .active
            .as_ref()
            .is_some_and(|id| !self.sessions.iter().any(|s| &s.id == id))
        {
            self.active = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unconfirmed_queue_recovers_to_draft_once_without_auto_resending() {
        let mut session = Session::new("project".into());
        let mut message = super::super::Message::new("user", "queued request".into());
        message.state = super::super::MessageState::Queued;
        session.messages.push(message);
        session.draft = "new draft".into();
        session.recover_unconfirmed_queue();
        session.recover_unconfirmed_queue();
        assert_eq!(session.draft, "queued request\n\nnew draft");
        assert_eq!(
            session.messages[0].state,
            super::super::MessageState::Unconfirmed
        );
    }
    #[test]
    fn discard_only_untouched_empty_sessions() {
        let blank = Session::new("project".into());
        let mut draft = Session::new("project".into());
        draft.draft = "unsent text".into();
        let mut db = Database {
            sessions: vec![blank, draft],
            ..Database::default()
        };
        db.discard_blank_sessions(None);
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].draft, "unsent text");
    }
    #[test]
    fn native_blank_persists_until_leaving_and_local_content_prevents_cleanup() {
        let mut session = Session::new("project".into());
        session.omp_session = Some("native.jsonl".into());
        assert!(
            !session.is_blank(),
            "persistence must retain the native binding"
        );
        assert!(session.is_untouched());
        for variant in 0..6 {
            let mut protected = session.clone();
            match variant {
                0 => protected.draft = "unsent text".into(),
                1 => protected.title = "renamed".into(),
                2 => protected.archived = true,
                3 => protected
                    .messages
                    .push(super::super::Message::new("user", "hello".into())),
                4 => protected.todos.push(serde_json::json!({"name":"work"})),
                _ => protected.agents.push(serde_json::json!({"id":"worker"})),
            }
            assert!(!protected.is_untouched());
        }
    }
}

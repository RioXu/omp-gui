use super::{Database, Session};
impl Session {
    pub fn recover_unconfirmed_queue(&mut self) {
        let mut drafts = Vec::new();
        for m in &mut self.messages {
            if m.state == super::MessageState::Queued {
                m.state = super::MessageState::Unconfirmed;
                drafts.push(m.text.clone());
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
        self.title == "新建会话"
            && self.omp_session.is_none()
            && self.messages.is_empty()
            && self.comments.is_empty()
            && self.draft.trim().is_empty()
            && self.snapshots.is_empty()
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
}

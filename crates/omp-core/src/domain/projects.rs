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
    /// Forget GUI metadata only. Native OMP sessions and project files are untouched.
    pub fn forget_project(&mut self, project: &crate::environment::ProjectLocation) {
        self.sessions.retain(|s| s.project_location() != *project);
        self.remote_projects.retain(|p| p != project);
        if project.environment == "local"
            && let crate::environment::HostPath::Local(path) = &project.path
        {
            self.projects.retain(|p| p != path);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|id| !self.sessions.iter().any(|s| &s.id == id))
        {
            self.active = None;
        }
    }

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
    fn forgetting_project_removes_archival_metadata_and_preserves_other_projects() {
        use crate::environment::ProjectLocation;
        let project = ProjectLocation::local("project".into());
        let mut archived = Session::new("project".into());
        archived.archived = true;
        archived.omp_session = Some("native-session.jsonl".into());
        let other = Session::new("other".into());
        let other_id = other.id.clone();
        let mut db = Database {
            projects: vec!["project".into(), "other".into()],
            active: Some(archived.id.clone()),
            sessions: vec![archived, Session::new("project".into()), other].into(),
            ..Default::default()
        };
        db.forget_project(&project);
        assert_eq!(db.projects, vec![std::path::PathBuf::from("other")]);
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].id, other_id);
        assert_eq!(db.active, None);
        let restored: Database =
            serde_json::from_value(serde_json::to_value(&db).unwrap()).unwrap();
        assert_eq!(restored.sessions.len(), 1);
        assert!(!restored.sessions.iter().any(|s| s.archived));
    }

    #[test]
    fn forgetting_remote_project_is_scoped_to_environment_and_profile() {
        use crate::environment::{HostPath, ProjectLocation};
        let project = ProjectLocation {
            environment: "server".into(),
            profile: Some("a".into()),
            path: HostPath::Posix("/project".into()),
        };
        let mut other_profile = project.clone();
        other_profile.profile = Some("b".into());
        let mut a = Session::new(Default::default());
        a.location = Some(project.clone());
        let mut b = Session::new(Default::default());
        b.location = Some(other_profile.clone());
        let mut db = Database {
            active: Some(b.id.clone()),
            sessions: vec![a, b].into(),
            remote_projects: vec![project.clone(), other_profile.clone()],
            ..Default::default()
        };
        db.forget_project(&project);
        assert_eq!(db.remote_projects, vec![other_profile.clone()]);
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].project_location(), other_profile);
        assert_eq!(db.active.as_ref(), Some(&db.sessions[0].id));
    }

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
            sessions: vec![blank, draft].into(),
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

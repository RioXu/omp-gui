use super::{Database, Session};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) fn project_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}
pub(crate) fn project_key(path: &Path) -> String {
    let resolved = project_path(path);
    let text = resolved.to_string_lossy().replace('/', "\\");
    let text = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    };
    text.trim_end_matches('\\').to_lowercase()
}
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
            && self.messages.is_empty()
            && self.comments.is_empty()
            && self.draft.trim().is_empty()
            && self.snapshots.is_empty()
    }
}
impl Database {
    /// Merge aliases of the same directory, never unrelated folders with the same name.
    pub fn normalize_projects(&mut self) {
        let mut paths = BTreeMap::new();
        let mut ordered = Vec::new();
        for path in self
            .projects
            .iter()
            .chain(self.sessions.iter().map(|s| &s.project))
        {
            let key = project_key(path);
            if let std::collections::btree_map::Entry::Vacant(entry) = paths.entry(key) {
                let normalized = project_path(path);
                entry.insert(normalized.clone());
                ordered.push(normalized);
            }
        }
        for session in &mut self.sessions {
            session.project = paths[&project_key(&session.project)].clone();
        }
        self.projects = ordered;
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
    fn windows_aliases_merge_but_same_basename_does_not() {
        assert_eq!(
            project_key(Path::new(r"D:\dev\czb_agent")),
            project_key(Path::new(r"\\?\D:\dev\czb_agent\"))
        );
        assert_ne!(
            project_key(Path::new(r"D:\one\czb_agent")),
            project_key(Path::new(r"D:\two\czb_agent"))
        );
        let mut db = Database {
            projects: vec![r"D:\dev\czb_agent".into(), r"\\?\D:\dev\czb_agent".into()],
            ..Database::default()
        };
        let mut a = Session::new(db.projects[0].clone());
        a.title = "first".into();
        let mut b = Session::new(db.projects[1].clone());
        b.title = "second".into();
        db.sessions = vec![a, b];
        db.normalize_projects();
        assert_eq!(db.projects.len(), 1);
        assert_eq!(db.sessions.len(), 2);
        assert_eq!(db.sessions[0].project, db.sessions[1].project);
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

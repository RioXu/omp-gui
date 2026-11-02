use super::*;
use omp_core::environment::ProjectLocation;

#[derive(Clone, PartialEq, Eq)]
struct SessionKey {
    id: String,
    title: String,
    project: ProjectLocation,
    updated: u64,
    archived: bool,
    deleted: bool,
}
impl SessionKey {
    fn matches(&self, s: &Session) -> bool {
        self.id == s.id
            && self.title == s.title
            && s.location.as_ref().map_or_else(
                || self.project.environment == "local" && self.project.profile.is_none()
                    && matches!(&self.project.path, omp_core::environment::HostPath::Local(path) if path == &s.project),
                |location| &self.project == location)
            && self.updated == s.updated
            && self.archived == s.archived
            && self.deleted == s.deleted
    }
}
#[derive(Clone)]
pub(super) struct Group {
    pub project: ProjectLocation,
    pub sessions: Vec<usize>,
}
#[derive(Default)]
pub(in crate::desktop::workspace) struct SidebarCache {
    keys: Vec<SessionKey>,
    projects: Vec<ProjectLocation>,
    archived: bool,
    groups: std::rc::Rc<Vec<Group>>,
}
impl SidebarCache {
    pub(super) fn groups(
        &mut self,
        db: &Database,
        projects: Vec<ProjectLocation>,
        archived: bool,
    ) -> std::rc::Rc<Vec<Group>> {
        if self.projects == projects
            && self.archived == archived
            && self.keys.len() == db.sessions.len()
            && self
                .keys
                .iter()
                .zip(db.sessions.iter())
                .all(|(key, s)| key.matches(s))
        {
            return self.groups.clone();
        }
        self.keys = db
            .sessions
            .iter()
            .map(|s| SessionKey {
                id: s.id.clone(),
                title: s.title.clone(),
                project: s.project_location(),
                updated: s.updated,
                archived: s.archived,
                deleted: s.deleted,
            })
            .collect();
        self.projects = projects;
        self.archived = archived;
        let mut by_project: HashMap<ProjectLocation, Vec<usize>> = HashMap::new();
        for (ix, s) in self
            .keys
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.deleted && s.archived == archived)
        {
            by_project.entry(s.project.clone()).or_default().push(ix);
        }
        self.groups = std::rc::Rc::new(
            self.projects
                .iter()
                .map(|project| {
                    let mut sessions = by_project.remove(project).unwrap_or_default();
                    sessions.sort_by_key(|ix| std::cmp::Reverse(self.keys[*ix].updated));
                    Group {
                        project: project.clone(),
                        sessions,
                    }
                })
                .collect(),
        );
        self.groups.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn groups_reuse_streaming_cache_but_refresh_navigation_changes() {
        let mut db = Database::default();
        let project = ProjectLocation::local("project".into());
        for updated in [1, 3, 2] {
            let mut session = Session::new("project".into());
            session.updated = updated;
            db.sessions.push(session);
        }
        let mut cache = SidebarCache::default();
        let groups = cache.groups(&db, vec![project.clone()], false);
        assert_eq!(groups[0].sessions, [1, 2, 0]);
        db.sessions[0]
            .messages
            .push(Message::new("assistant", "stream".into()));
        assert!(std::rc::Rc::ptr_eq(
            &groups,
            &cache.groups(&db, vec![project.clone()], false)
        ));
        db.sessions[0].title = "new title".into();
        assert!(!std::rc::Rc::ptr_eq(
            &groups,
            &cache.groups(&db, vec![project.clone()], false)
        ));
        db.sessions[1].archived = true;
        assert_eq!(
            cache.groups(&db, vec![project.clone()], false)[0].sessions,
            [2, 0]
        );
        assert_eq!(
            cache.groups(&db, vec![project.clone()], true)[0].sessions,
            [1]
        );
        db.sessions[1].deleted = true;
        assert!(
            cache.groups(&db, vec![project], true)[0]
                .sessions
                .is_empty()
        );
    }
}

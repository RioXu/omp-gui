//! Filesystem-backed Windows project identity and alias normalization.
use crate::domain::Database;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub fn project_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}
pub fn project_key(path: &Path) -> String {
    let resolved = project_path(path);
    if !cfg!(windows) {
        return resolved.to_string_lossy().trim_end_matches('/').to_owned();
    }
    let text = resolved.to_string_lossy().replace('/', "\\");
    let text = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    };
    text.trim_end_matches('\\').to_lowercase()
}
/// Merge aliases of the same directory, never unrelated folders with the same name.
pub fn normalize_workspace(db: &mut Database) {
    let mut paths = BTreeMap::new();
    let mut ordered = Vec::new();
    for path in db
        .projects
        .iter()
        .chain(db.sessions.iter().map(|s| &s.project))
    {
        let key = project_key(path);
        if let std::collections::btree_map::Entry::Vacant(entry) = paths.entry(key) {
            let normalized = project_path(path);
            entry.insert(normalized.clone());
            ordered.push(normalized);
        }
    }
    for session in &mut db.sessions {
        session.project = paths[&project_key(&session.project)].clone();
        if let Some(location) = &mut session.location
            && location.environment == "local"
        {
            location.path = crate::environment::HostPath::Local(session.project.clone());
        }
    }
    db.projects = ordered;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Session;
    #[test]
    #[cfg(not(windows))]
    fn posix_identity_is_case_sensitive() {
        assert_ne!(
            project_key(Path::new("/tmp/Project")),
            project_key(Path::new("/tmp/project"))
        );
        let mut db = Database::default();
        db.sessions.push(Session::new("/tmp/Project".into()));
        normalize_workspace(&mut db);
        assert_eq!(db.projects, vec![PathBuf::from("/tmp/Project")]);
    }
    #[test]
    #[cfg(windows)]
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
        normalize_workspace(&mut db);
        assert_eq!(db.projects.len(), 1);
        assert_eq!(db.sessions.len(), 2);
        assert_eq!(db.sessions[0].project, db.sessions[1].project);
    }
}

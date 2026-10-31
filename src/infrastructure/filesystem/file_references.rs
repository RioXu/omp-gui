//! Resolve explicit inline file references without treating code or URLs as links.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn resolve(root: &Path, text: &str) -> Option<PathBuf> {
    let text = text.trim();
    if text.is_empty() || text.contains(['\n', '\r', '\0']) || text.contains("://") {
        return None;
    }
    let path = Path::new(text);
    if path.has_root() && !path.is_absolute() {
        return None;
    }
    let resolved = fs::canonicalize(root.join(path)).ok()?;
    if !resolved.is_file() {
        return None;
    }
    if !path.is_absolute() && !resolved.starts_with(fs::canonicalize(root).ok()?) {
        return None;
    }
    Some(resolved)
}

/// Give relative/absolute aliases of a project file one tab identity.
pub(crate) fn document_path(root: &Path, path: &Path) -> PathBuf {
    let Ok(resolved) = fs::canonicalize(root.join(path)) else {
        return path.to_owned();
    };
    fs::canonicalize(root)
        .ok()
        .and_then(|root| resolved.strip_prefix(root).ok().map(Path::to_owned))
        .unwrap_or(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_existing_files_and_aliases_but_rejects_escapes_directories_and_code() {
        let temp = std::env::temp_dir().join(format!("omp-gui-links-{}", uuid::Uuid::new_v4()));
        let root = temp.join("project");
        fs::create_dir_all(root.join("目录")).unwrap();
        fs::write(root.join("目录/a b.rs"), "content").unwrap();
        fs::write(temp.join("outside.md"), "external").unwrap();
        let relative = PathBuf::from("目录/a b.rs");
        let full = fs::canonicalize(root.join(&relative)).unwrap();
        assert_eq!(resolve(&root, "目录/a b.rs"), Some(full.clone()));
        assert_eq!(resolve(&root, &full.to_string_lossy()), Some(full.clone()));
        assert_eq!(document_path(&root, &relative), document_path(&root, &full));
        assert!(resolve(&root, "../outside.md").is_none());
        assert!(resolve(&root, "目录").is_none());
        assert!(resolve(&root, "cargo test").is_none());
        assert!(resolve(&root, "https://example.com/file.rs").is_none());
        assert!(resolve(&root, "missing.rs").is_none());
        assert!(resolve(&root, &temp.join("outside.md").to_string_lossy()).is_some());
        let snapshot = crate::infrastructure::filesystem::project_files::document_snapshot(
            &root,
            &temp.join("outside.md"),
        )
        .unwrap();
        assert_eq!(snapshot.text, "external");
        assert!(
            crate::infrastructure::filesystem::project_files::read(
                &root,
                Path::new("../outside.md")
            )
            .is_err()
        );
        assert!(
            temp.starts_with(std::env::temp_dir())
                && temp
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-gui-links-")
        );
        fs::remove_dir_all(temp).unwrap();
    }
}

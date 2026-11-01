use super::*;
use crate::infrastructure::filesystem::{changes, document_store, project_files};
use anyhow::{Context, ensure};
use std::fs;

#[derive(Clone, Default)]
pub struct LocalHost;

impl DirectoryBrowser for LocalHost {
    fn browse(&self, path: Option<&HostPath>) -> Result<Directory> {
        let start = match path {
            Some(path) => path.local()?.to_owned(),
            None => std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(PathBuf::from)
                .unwrap_or(std::env::current_dir()?),
        };
        let root = crate::infrastructure::omp::sessions::plain_path(
            &fs::canonicalize(&start)
                .with_context(|| format!("无法打开目录：{}", start.display()))?,
        );
        ensure!(root.is_dir(), "请选择目录");
        let mut entries = Vec::new();
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.path().is_dir() {
                entries.push(DirectoryEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: HostPath::Local(entry.path()),
                });
            }
        }
        entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
        Ok(Directory {
            parent: root.parent().map(|p| HostPath::Local(p.to_owned())),
            path: HostPath::Local(root),
            entries,
        })
    }

    fn entered_path(&self, text: &str) -> Result<HostPath> {
        let path = PathBuf::from(text.trim());
        ensure!(path.is_absolute(), "请输入绝对目录路径");
        Ok(HostPath::Local(path))
    }
}

impl ProjectFiles for LocalHost {
    fn lists(&self, root: &HostPath) -> Result<(Vec<PathBuf>, Vec<ChangedFile>)> {
        let root = root.local()?;
        Ok((project_files::list(root)?, changes::list(root)?))
    }

    fn document(
        &self,
        root: &HostPath,
        path: &Path,
        changes: bool,
    ) -> Result<(Snapshot, Option<HostPath>)> {
        let root = root.local()?;
        let target = if changes {
            None
        } else {
            Some(HostPath::Local(document_store::target(root, path)?))
        };
        Ok((project_files::document_snapshot(root, path)?, target))
    }

    fn save(
        &self,
        root: &HostPath,
        path: &Path,
        target: &HostPath,
        expected: &str,
        text: &str,
    ) -> Result<()> {
        document_store::save(root.local()?, path, target.local()?, expected, text)
    }
}

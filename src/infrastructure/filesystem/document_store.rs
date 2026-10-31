//! Existing UTF-8 documents: checked atomic replacement, never blind overwrite.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) fn target(root: &Path, path: &Path) -> Result<PathBuf> {
    let target = if path.is_absolute() {
        fs::canonicalize(path)?
    } else {
        super::project_files::safe_path(root, path)?
    };
    ensure!(target.is_file(), "只能编辑普通文件");
    Ok(target)
}

pub(crate) fn save(
    root: &Path,
    path: &Path,
    opened_target: &Path,
    expected: &str,
    text: &str,
) -> Result<()> {
    let resolved = target(root, path).context("文件已移除或无法访问；编辑内容已保留")?;
    ensure!(
        resolved == opened_target,
        "文件指向已改变，未保存；编辑内容已保留"
    );
    let current = fs::read_to_string(&resolved)?;
    if current == text {
        return Ok(());
    }
    ensure!(
        current == expected,
        "文件已被外部修改，未覆盖磁盘内容；编辑内容已保留"
    );
    let metadata = fs::metadata(&resolved)?;
    ensure!(
        !metadata.permissions().readonly(),
        "文件为只读，未保存；编辑内容已保留"
    );
    let temp = resolved.with_file_name(format!(
        ".{}.omp-gui-{}.tmp",
        resolved
            .file_name()
            .context("文件名无效")?
            .to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        fs::set_permissions(&temp, metadata.permissions())?;
        file.sync_all()?;
        drop(file);
        ensure!(
            fs::read_to_string(&resolved)? == expected,
            "保存期间文件被外部修改，未覆盖磁盘内容"
        );
        fs::rename(&temp, &resolved).context("无法替换文件；编辑内容已保留")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("omp-gui-edit-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0.is_absolute()
                    && self.0.starts_with(std::env::temp_dir())
                    && self
                        .0
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("omp-gui-edit-")
            );
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn saves_unicode_and_rejects_stale_or_missing_disk_revisions() {
        let f = Fixture::new();
        let path = f.0.join("中文 file.txt");
        fs::write(&path, "old\r\n").unwrap();
        let opened = target(&f.0, Path::new("中文 file.txt")).unwrap();
        save(&f.0, &path, &opened, "old\r\n", "new\r\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\r\n");
        assert!(save(&f.0, &path, &opened, "old\r\n", "overwrite").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\r\n");
        assert_eq!(
            fs::read_dir(&f.0).unwrap().count(),
            1,
            "no temporary files left"
        );
        fs::remove_file(&path).unwrap();
        assert!(save(&f.0, &path, &opened, "new\r\n", "recreate").is_err());
        assert!(!path.exists());
    }
    #[test]
    fn checks_the_opened_target_and_allows_explicit_external_documents() {
        let f = Fixture::new();
        let project = f.0.join("project");
        fs::create_dir(&project).unwrap();
        let external = f.0.join("external.txt");
        fs::write(&external, "original").unwrap();
        assert!(target(&project, Path::new("../external.txt")).is_err());
        let opened = target(&project, &external).unwrap();
        assert!(save(&project, &external, &project, "original", "wrong target").is_err());
        save(&project, &external, &opened, "original", "edited").unwrap();
        assert_eq!(fs::read_to_string(&external).unwrap(), "edited");
    }
    #[test]
    fn readonly_files_are_preserved_on_save_failure() {
        let f = Fixture::new();
        let path = f.0.join("readonly.txt");
        fs::write(&path, "original").unwrap();
        let opened = target(&f.0, &path).unwrap();
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = save(&f.0, &path, &opened, "original", "changed");
        fs::set_permissions(&path, original).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "original");
    }
}

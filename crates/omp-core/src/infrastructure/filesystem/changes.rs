//! Git changes with status retained for consistent presentation.
use crate::infrastructure::process::hidden_command;
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Conflicted,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

pub fn list(root: &Path) -> anyhow::Result<Vec<ChangedFile>> {
    let output = hidden_command("git")
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        // An ordinary directory has files, but no Git changes.
        return Ok(Vec::new());
    }
    let prefix = hidden_command("git")
        .args(["rev-parse", "--show-prefix"])
        .current_dir(root)
        .output()?;
    ensure!(prefix.status.success(), "无法读取 Git 项目路径");
    parse(
        &output.stdout,
        std::str::from_utf8(&prefix.stdout)?.trim_end(),
    )
}

fn parse(bytes: &[u8], prefix: &str) -> anyhow::Result<Vec<ChangedFile>> {
    let mut records = bytes.split(|b| *b == 0).filter(|s| !s.is_empty());
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        ensure!(
            record.len() >= 4 && record[2] == b' ',
            "无效的 Git 状态记录"
        );
        let status = &record[..2];
        if status.contains(&b'R') || status.contains(&b'C') {
            records.next().context("Git 重命名记录缺少源路径")?;
        }
        let path = std::str::from_utf8(&record[3..]).context("Git 文件名不是 UTF-8")?;
        let Some(path) = path.strip_prefix(prefix) else {
            continue;
        };
        let kind = if status.contains(&b'U') || status == b"AA" || status == b"DD" {
            ChangeKind::Conflicted
        } else if status.contains(&b'D') {
            ChangeKind::Deleted
        } else if status.contains(&b'R') {
            ChangeKind::Renamed
        } else if status.contains(&b'A') || status.contains(&b'?') || status.contains(&b'C') {
            ChangeKind::Added
        } else {
            ChangeKind::Modified
        };
        files.push(ChangedFile {
            path: path.into(),
            kind,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.truncate(5000);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_keep_unicode_spaces_and_rename_destinations() {
        let files = parse(" M src/修改 文件.rs\0?? 新 文件.md\0D  deleted.rs\0R  src/新名.rs\0src/旧名.rs\0UU conflict.rs\0".as_bytes(), "").unwrap();
        for (path, kind) in [
            ("src/修改 文件.rs", ChangeKind::Modified),
            ("新 文件.md", ChangeKind::Added),
            ("deleted.rs", ChangeKind::Deleted),
            ("src/新名.rs", ChangeKind::Renamed),
            ("conflict.rs", ChangeKind::Conflicted),
        ] {
            assert!(files.contains(&ChangedFile {
                path: path.into(),
                kind
            }));
        }
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn project_subdirectories_use_relative_paths() {
        let files = parse(b" M src/nested/a.rs\0 M sibling.rs\0", "src/").unwrap();
        assert_eq!(
            files,
            vec![ChangedFile {
                path: "nested/a.rs".into(),
                kind: ChangeKind::Modified
            }]
        );
        assert!(parse(b"R  new.rs\0", "").is_err());
    }

    #[test]
    fn git_status_tracks_added_modified_deleted_and_renamed_files() {
        let temporary = std::env::temp_dir();
        let root = temporary.join(format!("omp-gui-changes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        let git = |args: &[&str]| {
            let out = hidden_command("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q"]);
        for name in ["modified.rs", "deleted.rs", "old.rs", "unchanged.rs"] {
            std::fs::write(root.join("src").join(name), "original\n").unwrap();
        }
        // An unborn repository still exposes untracked files as additions.
        assert!(
            list(&root)
                .unwrap()
                .iter()
                .all(|c| c.kind == ChangeKind::Added)
        );
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
        ]);
        std::fs::write(root.join("src/modified.rs"), "modified\n").unwrap();
        std::fs::remove_file(root.join("src/deleted.rs")).unwrap();
        git(&["mv", "src/old.rs", "src/renamed.rs"]);
        std::fs::write(root.join("src/新增 文件.rs"), "new\n").unwrap();
        let changes = list(&root.join("src")).unwrap();
        assert_eq!(changes.len(), 4);
        for (path, kind) in [
            ("modified.rs", ChangeKind::Modified),
            ("deleted.rs", ChangeKind::Deleted),
            ("renamed.rs", ChangeKind::Renamed),
            ("新增 文件.rs", ChangeKind::Added),
        ] {
            assert!(changes.contains(&ChangedFile {
                path: path.into(),
                kind
            }));
        }
        assert_eq!(root.parent(), Some(temporary.as_path()));
        std::fs::remove_dir_all(root).unwrap();
    }
}

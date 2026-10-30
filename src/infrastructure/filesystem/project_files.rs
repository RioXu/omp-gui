use crate::domain::Snapshot;
use crate::infrastructure::process::hidden_command;
use anyhow::Context;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn safe_path(root: &Path, relative: &Path) -> anyhow::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let p = fs::canonicalize(root.join(relative))?;
    anyhow::ensure!(p.starts_with(&root), "文件不在当前项目内");
    Ok(p)
}
pub(crate) fn read(root: &Path, path: &Path) -> anyhow::Result<String> {
    let p = safe_path(root, path)?;
    anyhow::ensure!(
        fs::metadata(&p)?.len() <= 2 * 1024 * 1024,
        "文件超过 2 MB，暂不在界面中预览"
    );
    fs::read_to_string(p).context("暂不支持二进制或非 UTF-8 文件")
}
pub(crate) fn snapshot(root: &Path, path: &Path) -> anyhow::Result<Snapshot> {
    let text = match read(root, path) {
        Ok(s) => s,
        Err(e) => {
            if root.join(path).exists() {
                return Err(e);
            }
            String::new()
        }
    };
    let output = hidden_command("git")
        .args([
            "show",
            &format!("HEAD:{}", path.to_string_lossy().replace('\\', "/")),
        ])
        .current_dir(root)
        .output();
    let before = output
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    Ok(Snapshot::new(path.to_owned(), before, text))
}
pub(crate) fn list(root: &Path, changed: bool) -> anyhow::Result<Vec<PathBuf>> {
    let args = if changed {
        vec!["diff", "--name-only", "-z", "HEAD", "--"]
    } else {
        vec![
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ]
    };
    let out = hidden_command("git")
        .args(args)
        .current_dir(root)
        .output()?;
    let mut paths = Vec::new();
    if out.status.success() {
        paths.extend(split_paths(&out.stdout));
        if changed {
            let untracked = hidden_command("git")
                .args(["ls-files", "-z", "--others", "--exclude-standard"])
                .current_dir(root)
                .output()?;
            paths.extend(split_paths(&untracked.stdout));
        }
    } else {
        walk(root, root, &mut paths, 0)?;
    }
    paths.sort();
    paths.dedup();
    paths.truncate(5000);
    Ok(paths)
}
fn split_paths(b: &[u8]) -> Vec<PathBuf> {
    b.split(|c| *c == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok().map(PathBuf::from))
        .collect()
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> anyhow::Result<()> {
    if depth > 12 || out.len() >= 5000 {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let e = entry?;
        let t = e.file_type()?;
        if t.is_symlink() {
            continue;
        }
        if [".git", "target", "node_modules", ".venv"]
            .contains(&e.file_name().to_string_lossy().as_ref())
        {
            continue;
        }
        if t.is_dir() {
            walk(root, &e.path(), out, depth + 1)?;
        } else {
            out.push(e.path().strip_prefix(root)?.to_path_buf());
        }
        if out.len() >= 5000 {
            break;
        }
    }
    Ok(())
}

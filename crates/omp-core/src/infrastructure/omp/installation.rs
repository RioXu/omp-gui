//! Resolve OMP in the executing host's environment; never assume an install folder.
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

/// Persist the command name for automatic discovery, not a machine-specific path.
pub fn executable() -> String {
    "omp".into()
}

pub fn resolve(override_path: &str) -> Result<PathBuf> {
    let command = override_path.trim();
    let command = if command.is_empty() { "omp" } else { command };
    let path = Path::new(command);
    if path.is_absolute() || path.components().count() > 1 {
        ensure!(path.is_absolute(), "OMP 覆盖路径必须是绝对路径");
        ensure!(path.is_file(), "OMP 可执行文件不存在：{}", path.display());
        return Ok(path.to_owned());
    }
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let extensions = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""][..]
    } else {
        &[""][..]
    };
    find(command, std::env::split_paths(&paths), extensions)
        .with_context(|| format!("当前环境 PATH 中未找到 {command}；请在设置中填写 OMP 绝对路径"))
}

fn find(
    command: &str,
    paths: impl Iterator<Item = PathBuf>,
    extensions: &[&str],
) -> Option<PathBuf> {
    for directory in paths.filter(|p| p.is_absolute()) {
        for extension in extensions {
            let candidate = directory.join(format!("{command}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_follows_path_order_and_ignores_relative_search_directories() {
        let root = std::env::temp_dir().join(format!("omp-path-{}", uuid::Uuid::new_v4()));
        let a = root.join("a 空格");
        let b = root.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("omp.exe"), "a").unwrap();
        std::fs::write(b.join("omp.exe"), "b").unwrap();
        assert_eq!(
            find(
                "omp",
                vec![PathBuf::from("."), a.clone(), b.clone()].into_iter(),
                &[".exe"]
            ),
            Some(a.join("omp.exe"))
        );
        assert_eq!(find("missing", vec![a, b].into_iter(), &[".exe"]), None);
        assert!(resolve("./omp").is_err());
        assert!(
            root.is_absolute()
                && root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-path-")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

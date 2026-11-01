use super::*;

/// Windows extended paths must not change OMP's cwd bucket or lock identity.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    let text = text.replace('/', "\\");
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}").into();
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).into()
}
pub(super) fn bucket(cwd: &Path, home: &Path, temp: &Path) -> String {
    fn encode(s: &str) -> String {
        s.replace(['/', '\\', ':'], "-")
    }
    for (root, prefix) in [(temp, "-tmp"), (home, "-")] {
        if let Ok(relative) = cwd.strip_prefix(root) {
            let encoded = encode(&relative.to_string_lossy());
            return if encoded.is_empty() {
                prefix.into()
            } else if prefix.ends_with('-') {
                format!("{prefix}{encoded}")
            } else {
                format!("{prefix}-{encoded}")
            };
        }
    }
    format!(
        "--{}--",
        encode(cwd.to_string_lossy().trim_start_matches(['/', '\\']))
    )
}
pub(super) fn validate_path(path: &Path, root: &Path) -> Result<()> {
    let root = plain_path(&fs::canonicalize(root)?);
    let parent = plain_path(&fs::canonicalize(path.parent().context("会话目录无效")?)?);
    ensure!(parent.starts_with(&root), "会话路径不在当前 OMP 存储目录内");
    if path.try_exists()? {
        ensure!(
            plain_path(&fs::canonicalize(path)?).starts_with(root),
            "会话文件指向了存储目录之外，未删除"
        );
    }
    Ok(())
}

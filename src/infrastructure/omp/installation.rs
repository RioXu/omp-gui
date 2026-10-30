//! Locate the installed executable when creating a new workspace.
pub(crate) fn executable() -> String {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("omp/omp.exe"))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "omp".into())
}

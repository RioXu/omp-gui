use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A remote POSIX path is never passed through the client's native path resolver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum HostPath {
    Local(PathBuf),
    Posix(String),
}

impl HostPath {
    pub fn posix(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        ensure!(
            path.starts_with('/') && !path.contains('\0'),
            "远端目录必须是 POSIX 绝对路径"
        );
        Ok(Self::Posix(path))
    }

    pub fn local(&self) -> Result<&Path> {
        match self {
            Self::Local(path) => Ok(path),
            Self::Posix(_) => anyhow::bail!("远端路径不能交给本地文件系统"),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::Local(path) => path.to_string_lossy().into_owned(),
            Self::Posix(path) => path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posix_paths_retain_case_backslashes_and_symlink_parent_coordinates() {
        let path = HostPath::posix("/home/Test/link/../a\\b").unwrap();
        assert_eq!(path.text(), "/home/Test/link/../a\\b");
        assert!(path.local().is_err());
        assert_ne!(path, HostPath::posix("/home/test/link/../a\\b").unwrap());
        assert!(HostPath::posix("../project").is_err());
        assert!(HostPath::posix("/bad\0path").is_err());
    }
}

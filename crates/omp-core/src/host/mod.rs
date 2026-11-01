//! Operating-host interfaces. Call blocking adapters only from worker threads.
pub mod configuration;
mod environment;
pub use configuration::{ConfigAction, Configuration};
mod local;
mod watch;
use crate::{
    domain::Snapshot, environment::HostPath, infrastructure::filesystem::changes::ChangedFile,
};
use anyhow::Result;
pub use environment::{ExecutionHost, NativeStore};
pub use local::LocalHost;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
pub use watch::{FileChange, FileWatch};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Directory {
    pub path: HostPath,
    pub parent: Option<HostPath>,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: HostPath,
}

pub trait DirectoryBrowser: Send + Sync {
    fn browse(&self, path: Option<&HostPath>) -> Result<Directory>;
    fn entered_path(&self, text: &str) -> Result<HostPath>;
}

/// Project-relative paths in results are display coordinates; the adapter owns resolution.
pub trait ProjectFiles: Send + Sync {
    fn lists(&self, root: &HostPath) -> Result<(Vec<PathBuf>, Vec<ChangedFile>)>;
    fn document(
        &self,
        root: &HostPath,
        path: &Path,
        changes: bool,
    ) -> Result<(Snapshot, Option<HostPath>)>;
    fn save(
        &self,
        root: &HostPath,
        path: &Path,
        target: &HostPath,
        expected: &str,
        text: &str,
    ) -> Result<()>;
}

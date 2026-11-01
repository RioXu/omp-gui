//! Host identities and paths are independent of the desktop platform.
mod paths;
pub use paths::HostPath;
pub fn file_label(path: &std::path::Path, posix: bool) -> String {
    if posix {
        path.to_string_lossy()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    } else {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}
pub fn display_path(path: &std::path::Path, posix: bool) -> String {
    let text = path.to_string_lossy();
    if posix {
        text.into_owned()
    } else {
        text.replace('\\', "/")
    }
}
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub connection: Connection,
    #[serde(default)]
    pub omp_override: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub helper: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Connection {
    Local,
    Ssh { host: String },
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            id: "local".into(),
            name: "本地".into(),
            connection: Connection::Local,
            omp_override: None,
            profile: None,
            helper: None,
        }
    }
}
impl Environment {
    /// Host/profile changes create a new ownership boundary, never retarget history.
    pub fn same_target(&self, other: &Self) -> bool {
        self.connection == other.connection && self.profile == other.profile
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectLocation {
    pub environment: String,
    pub profile: Option<String>,
    pub path: HostPath,
}

impl ProjectLocation {
    pub fn local(path: std::path::PathBuf) -> Self {
        Self {
            environment: "local".into(),
            profile: None,
            path: HostPath::Local(path),
        }
    }
    pub fn label(&self) -> String {
        match &self.path {
            HostPath::Local(path) => path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            HostPath::Posix(path) => path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("/")
                .to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_edits_preserve_ownership_only_for_the_same_host_and_profile() {
        let mut a = Environment {
            connection: Connection::Ssh { host: "dev".into() },
            ..Default::default()
        };
        a.id = "a".into();
        let mut b = a.clone();
        b.name = "renamed".into();
        b.omp_override = Some("/new/omp".into());
        assert!(a.same_target(&b));
        b.profile = Some("/profile/other".into());
        assert!(!a.same_target(&b));
        b.profile = None;
        b.connection = Connection::Ssh {
            host: "other".into(),
        };
        assert!(!a.same_target(&b));
        assert_eq!(
            file_label(std::path::Path::new("dir/a\\b.rs"), true),
            "a\\b.rs"
        );
        assert_eq!(
            display_path(std::path::Path::new("dir/a\\b.rs"), true),
            "dir/a\\b.rs"
        );
    }
}

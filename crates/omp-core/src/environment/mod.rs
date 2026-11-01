//! Host identities and paths are independent of the desktop platform.
mod paths;
pub use paths::HostPath;
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLocation {
    pub environment: String,
    pub profile: Option<String>,
    pub path: HostPath,
}

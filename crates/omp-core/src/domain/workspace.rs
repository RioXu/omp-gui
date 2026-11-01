use super::Session;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Database {
    pub version: u32,
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    pub active: Option<String>,
    pub omp_path: String,
    pub theme: String,
    #[serde(default = "sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default = "panel_width")]
    pub panel_width: f32,
    #[serde(default = "local_environments")]
    pub environments: Vec<crate::environment::Environment>,
    #[serde(default = "local_environment")]
    pub active_environment: String,
    #[serde(default)]
    pub remote_projects: Vec<crate::environment::ProjectLocation>,
}
impl Default for Database {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            sessions: vec![],
            projects: vec![],
            active: None,
            omp_path: "omp".into(),
            theme: "system".into(),
            sidebar_width: sidebar_width(),
            panel_width: panel_width(),
            environments: local_environments(),
            active_environment: local_environment(),
            remote_projects: Vec::new(),
        }
    }
}
fn local_environment() -> String {
    "local".into()
}
fn local_environments() -> Vec<crate::environment::Environment> {
    vec![Default::default()]
}
impl Database {
    pub const VERSION: u32 = 2;
    pub fn remove_session(&mut self, id: &str) {
        self.sessions.retain(|s| s.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
    }
}
fn sidebar_width() -> f32 {
    240.0
}
fn panel_width() -> f32 {
    460.0
}

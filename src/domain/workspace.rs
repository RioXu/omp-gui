use super::Session;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Database {
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
}
impl Default for Database {
    fn default() -> Self {
        Self {
            version: 1,
            sessions: vec![],
            projects: vec![],
            active: None,
            omp_path: "omp".into(),
            theme: "system".into(),
            sidebar_width: sidebar_width(),
            panel_width: panel_width(),
        }
    }
}
impl Database {
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

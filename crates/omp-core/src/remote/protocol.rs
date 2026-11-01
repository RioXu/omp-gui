use crate::{domain::Session, environment::HostPath};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 3;
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub protocol: u32,
    pub id: String,
    pub operation: Operation,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub protocol: u32,
    pub id: String,
    pub result: Result<Value, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Operation {
    QueryOperation {
        id: String,
    },
    ChildTranscript {
        primary: String,
        agent: String,
        path: Option<String>,
    },
    ResolveReferences {
        root: HostPath,
        candidates: Vec<String>,
    },
    Upload {
        session: String,
        id: String,
        name: String,
        data: String,
        sha256: String,
    },
    Configure {
        action: Box<crate::host::ConfigAction>,
    },
    Hello {
        omp: String,
    },
    Browse {
        path: Option<HostPath>,
    },
    Lists {
        root: HostPath,
    },
    Document {
        root: HostPath,
        path: String,
        changes: bool,
    },
    SaveDocument {
        root: HostPath,
        path: String,
        target: HostPath,
        expected: String,
        text: String,
    },
    ScanSessions {
        omp: String,
        projects: Vec<HostPath>,
        known: Vec<crate::infrastructure::omp::sessions::Registration>,
    },
    ScanSessionIndex {
        omp: String,
        projects: Vec<HostPath>,
        known: Vec<crate::infrastructure::omp::sessions::Registration>,
    },
    ReadSessionHistory {
        omp: String,
        cwd: HostPath,
        path: String,
    },
    CreateSession {
        omp: String,
        project: HostPath,
        title: String,
    },
    DeleteSession {
        omp: String,
        session: Box<Session>,
        empty_only: bool,
    },
    SessionAvailable {
        omp: String,
        cwd: HostPath,
        id: String,
    },
    Catalog {
        omp: String,
        cwd: HostPath,
    },
    Command {
        omp: String,
        cwd: HostPath,
        args: Vec<String>,
    },
    ReadConfig {
        path: HostPath,
    },
    SaveConfig {
        path: HostPath,
        expected: String,
        text: String,
    },
}

impl Operation {
    pub fn mutates(&self) -> bool {
        match self {
            Self::SaveDocument { .. }
            | Self::Upload { .. }
            | Self::CreateSession { .. }
            | Self::DeleteSession { .. }
            | Self::SaveConfig { .. } => true,
            Self::Command { args, .. } => command_mutates(args),
            Self::Configure { action } => {
                use crate::host::ConfigAction as C;
                match action.as_ref() {
                    C::Save { .. }
                    | C::SetProject { .. }
                    | C::SaveAgent { .. }
                    | C::SaveRoles { .. }
                    | C::SaveProvider { .. }
                    | C::SaveScoped { .. }
                    | C::ImportKey { .. } => true,
                    C::Command { args, .. } => command_mutates(args),
                    _ => false,
                }
            }
            _ => false,
        }
    }
}
fn command_mutates(args: &[String]) -> bool {
    !matches!(
        args.first().map(String::as_str),
        Some("--version" | "--list-models")
    ) && !(args.first().is_some_and(|s| s == "config")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "path" | "get" | "list" | "show")))
}

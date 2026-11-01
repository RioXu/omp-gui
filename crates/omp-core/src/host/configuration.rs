//! Configuration effects execute on the selected host; forms keep pure draft logic.
use super::ExecutionHost;
use crate::{
    environment::HostPath,
    infrastructure::configuration::{
        authoring::{self, AgentDraft, RoleDraft},
        providers::{self, Providers, ServiceCatalog},
        settings::{self, Catalog},
    },
    remote::protocol::Operation,
};
use anyhow::Result;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Configuration {
    pub host: ExecutionHost,
    pub omp: String,
    pub cwd: HostPath,
}
impl Configuration {
    pub fn target_label(&self) -> String {
        match &self.host {
            ExecutionHost::Local => "本地 OMP".into(),
            ExecutionHost::Remote(remote) => format!(
                "SSH：{} · {}",
                remote.host,
                remote.profile.as_deref().unwrap_or("默认 profile")
            ),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ConfigAction {
    FindConfig {
        base: String,
        names: Vec<String>,
    },
    Catalog {
        omp: String,
        cwd: String,
    },
    Read {
        path: String,
    },
    Save {
        path: String,
        expected: String,
        text: String,
    },
    SetProject {
        base: String,
        key: String,
        value: Option<Value>,
    },
    LoadAgent {
        path: String,
    },
    SaveAgent {
        draft: Box<AgentDraft>,
        base: String,
        name: String,
        text: String,
    },
    LoadRoles {
        base: String,
        effective: Value,
    },
    SaveRoles {
        draft: Box<RoleDraft>,
        values: Map<String, Value>,
    },
    Models {
        omp: String,
        cwd: String,
    },
    Providers {
        root: String,
    },
    SaveProvider {
        store: Box<Providers>,
        id: String,
        value: Option<Value>,
    },
    Services {
        omp: String,
        cwd: String,
    },
    DiscoverModels {
        base: String,
        key: String,
        api: String,
    },
    SaveScoped {
        omp: String,
        cwd: String,
        root: String,
        project: bool,
        key: String,
        value: Value,
    },
    ImportKey {
        omp: String,
        cwd: String,
        provider: String,
        key: String,
    },
    Command {
        omp: String,
        cwd: String,
        args: Vec<String>,
    },
    ResolveOmp {
        omp: String,
    },
}

pub fn execute(action: ConfigAction) -> Result<Value> {
    match action {
        ConfigAction::FindConfig { base, names } => {
            let base = Path::new(&base);
            let path = names
                .iter()
                .map(|name| base.join(name))
                .find(|path| path.is_file())
                .unwrap_or_else(|| {
                    base.join(names.first().map(String::as_str).unwrap_or("config.yml"))
                });
            Ok(serde_json::to_value(path)?)
        }
        ConfigAction::Catalog { omp, cwd } => Ok(serde_json::to_value(settings::catalog(
            &omp,
            Path::new(&cwd),
        )?)?),
        ConfigAction::Read { path } => Ok(serde_json::to_value(settings::read_document(
            Path::new(&path),
        )?)?),
        ConfigAction::Save {
            path,
            expected,
            text,
        } => {
            settings::save_document(Path::new(&path), &expected, &text)?;
            Ok(Value::Null)
        }
        ConfigAction::SetProject { base, key, value } => {
            settings::set_project(&settings::config_path(Path::new(&base)), &key, value)?;
            Ok(Value::Null)
        }
        ConfigAction::LoadAgent { path } => {
            Ok(serde_json::to_value(AgentDraft::load(Path::new(&path))?)?)
        }
        ConfigAction::SaveAgent {
            draft,
            base,
            name,
            text,
        } => {
            draft.save(Path::new(&base), &name, &text)?;
            Ok(Value::Null)
        }
        ConfigAction::LoadRoles { base, effective } => Ok(serde_json::to_value(RoleDraft::load(
            settings::config_path(Path::new(&base)),
            effective,
        )?)?),
        ConfigAction::SaveRoles { draft, values } => {
            draft.save(values)?;
            Ok(Value::Null)
        }
        ConfigAction::Models { omp, cwd } => Ok(serde_json::to_value(
            authoring::available_models(&omp, Path::new(&cwd))?,
        )?),
        ConfigAction::Providers { root } => {
            Ok(serde_json::to_value(Providers::load(Path::new(&root))?)?)
        }
        ConfigAction::SaveProvider { store, id, value } => {
            store.save(&id, value.as_ref())?;
            Ok(Value::Null)
        }
        ConfigAction::Services { omp, cwd } => Ok(serde_json::to_value(ServiceCatalog::load(
            &omp,
            Path::new(&cwd),
        ))?),
        ConfigAction::DiscoverModels { base, key, api } => Ok(serde_json::to_value(
            providers::discover_models(&base, &key, &api)?,
        )?),
        ConfigAction::SaveScoped {
            omp,
            cwd,
            root,
            project,
            key,
            value,
        } => {
            providers::save_scoped(
                &omp,
                Path::new(&cwd),
                Path::new(&root),
                project,
                &key,
                value,
            )?;
            Ok(Value::Null)
        }
        ConfigAction::ImportKey {
            omp,
            cwd,
            provider,
            key,
        } => {
            providers::import_key(&omp, Path::new(&cwd), &provider, &key)?;
            Ok(Value::Null)
        }
        ConfigAction::Command { omp, cwd, args } => Ok(serde_json::to_value(settings::command(
            &omp,
            Path::new(&cwd),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?)?),
        ConfigAction::ResolveOmp { omp } => Ok(serde_json::to_value(
            crate::infrastructure::omp::installation::resolve(&omp)?,
        )?),
    }
}

impl Configuration {
    pub fn call<T: DeserializeOwned>(&self, action: ConfigAction) -> Result<T> {
        match &self.host {
            ExecutionHost::Local => Ok(serde_json::from_value(execute(action)?)?),
            ExecutionHost::Remote(remote) => remote.call(Operation::Configure {
                action: Box::new(action),
            }),
        }
    }
    pub fn join(&self, base: &Path, name: &str) -> PathBuf {
        match self.host {
            ExecutionHost::Local => base.join(name),
            ExecutionHost::Remote(_) => format!(
                "{}/{}",
                base.to_string_lossy().trim_end_matches('/'),
                name.trim_start_matches('/')
            )
            .into(),
        }
    }
    pub fn catalog(&self) -> Result<Catalog> {
        self.call(ConfigAction::Catalog {
            omp: self.omp.clone(),
            cwd: self.cwd.text(),
        })
    }
    pub fn command(&self, args: &[&str]) -> Result<String> {
        self.call(ConfigAction::Command {
            omp: self.omp.clone(),
            cwd: self.cwd.text(),
            args: args.iter().map(|s| (*s).into()).collect(),
        })
    }
    pub fn models(&self) -> Result<Vec<Value>> {
        self.call(ConfigAction::Models {
            omp: self.omp.clone(),
            cwd: self.cwd.text(),
        })
    }
    pub fn services(&self) -> Result<ServiceCatalog> {
        self.call(ConfigAction::Services {
            omp: self.omp.clone(),
            cwd: self.cwd.text(),
        })
    }
    pub fn providers(&self, root: &Path) -> Result<Providers> {
        self.call(ConfigAction::Providers {
            root: root.to_string_lossy().into_owned(),
        })
    }
    pub fn read(&self, path: &Path) -> Result<String> {
        self.call(ConfigAction::Read {
            path: path.to_string_lossy().into_owned(),
        })
    }
    pub fn save(&self, path: &Path, expected: &str, text: &str) -> Result<()> {
        self.call(ConfigAction::Save {
            path: path.to_string_lossy().into_owned(),
            expected: expected.into(),
            text: text.into(),
        })
    }
    pub fn set_project(&self, base: &Path, key: &str, value: Option<Value>) -> Result<()> {
        self.call(ConfigAction::SetProject {
            base: base.to_string_lossy().into_owned(),
            key: key.into(),
            value,
        })
    }
    pub fn save_scoped(&self, root: &Path, project: bool, key: &str, value: Value) -> Result<()> {
        self.call(ConfigAction::SaveScoped {
            omp: self.omp.clone(),
            cwd: self.cwd.text(),
            root: root.to_string_lossy().into_owned(),
            project,
            key: key.into(),
            value,
        })
    }
}

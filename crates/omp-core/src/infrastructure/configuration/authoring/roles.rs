use crate::infrastructure::configuration::settings;
use anyhow::ensure;
use serde_json::{Map, Value};
use std::{collections::BTreeSet, path::PathBuf};

pub fn role_names(effective: &Value) -> Vec<String> {
    let mut names = [
        "default",
        "smol",
        "slow",
        "plan",
        "commit",
        "vision",
        "advisor",
        "web",
        "image",
        "speech",
        "dictation",
        "judge",
        "tiny",
        "memory",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    if let Some(map) = effective.as_object() {
        names.extend(map.keys().cloned());
    }
    names.into_iter().collect()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RoleDraft {
    pub values: Map<String, Value>,
    pub effective: Value,
    pub path: PathBuf,
    original: String,
}
impl RoleDraft {
    pub fn load(path: PathBuf, effective: Value) -> anyhow::Result<Self> {
        let original = settings::read_document(&path)?;
        let yaml: Value = if original.trim().is_empty() {
            Value::Null
        } else {
            serde_yaml_ng::from_str(&original)?
        };
        let roles = yaml.get("modelRoles");
        ensure!(
            roles.is_none_or(|v| v.is_object() || v.is_null()),
            "modelRoles 必须是角色到模型的映射"
        );
        Ok(Self {
            values: roles
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            effective,
            path,
            original,
        })
    }
    pub fn save(&self, values: Map<String, Value>) -> anyhow::Result<()> {
        let text = settings::patch_yaml(&self.original, "modelRoles", Some(Value::Object(values)))?;
        settings::save_document(&self.path, &self.original, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scoped_role_form_preserves_other_roles_priorities_and_configuration() {
        let root = std::env::temp_dir().join(format!("omp-gui-role-form-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let global = root.join("global.yml");
        let project = root.join("project.yml");
        std::fs::write(&global, "modelRoles: {default: provider/global}\n").unwrap();
        std::fs::write(&project, "# preserved\nfuture: {value: 1}\nmodelRoles:\n  review: [provider/first:high, provider/second]\n  web: web/exa\n").unwrap();
        let draft = RoleDraft::load(
            project.clone(),
            json!({"default":"provider/global","review":["provider/first:high","provider/second"]}),
        )
        .unwrap();
        let mut values = draft.values.clone();
        values.insert("default".into(), json!("deepseek/flash:max"));
        draft.save(values).unwrap();
        let saved = std::fs::read_to_string(&project).unwrap();
        let parsed: Value = serde_yaml_ng::from_str(&saved).unwrap();
        assert!(saved.contains("# preserved"));
        assert_eq!(parsed["future"]["value"], 1);
        assert_eq!(parsed["modelRoles"]["web"], "web/exa");
        assert_eq!(
            parsed["modelRoles"]["review"],
            json!(["provider/first:high", "provider/second"])
        );
        assert_eq!(parsed["modelRoles"]["default"], "deepseek/flash:max");
        assert_eq!(
            std::fs::read_to_string(global).unwrap(),
            "modelRoles: {default: provider/global}\n"
        );
        assert!(
            draft.save(draft.values.clone()).is_err(),
            "stale forms must not replace newer values"
        );
    }
}

use crate::infrastructure::configuration::settings;
use anyhow::{Context, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use yaml_edit::{Document, path::YamlPath};

#[derive(Clone, Default)]
pub(crate) struct AgentDraft {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub model: Option<String>,
    original: String,
    path: Option<PathBuf>,
    frontmatter: String,
}

impl AgentDraft {
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let original = settings::read_document(path)?;
        let mut lines = original.split_inclusive('\n');
        ensure!(
            lines
                .next()
                .is_some_and(|line| line.trim().trim_start_matches('\u{feff}') == "---"),
            "子智能体文件缺少 YAML 头部"
        );
        let start = original.find('\n').context("子智能体头部不完整")? + 1;
        let mut offset = start;
        for line in lines {
            if line.trim() == "---" {
                let frontmatter = original[start..offset].to_owned();
                let values: Value = serde_yaml_ng::from_str(&frontmatter)?;
                ensure!(values.is_object(), "子智能体头部必须是对象");
                return Ok(Self {
                    name: values["name"].as_str().unwrap_or_default().into(),
                    description: values["description"].as_str().unwrap_or_default().into(),
                    prompt: original[offset + line.len()..]
                        .trim_start_matches(['\r', '\n'])
                        .into(),
                    model: values["model"]
                        .as_str()
                        .or_else(|| values["model"].as_array()?.first()?.as_str())
                        .map(str::to_owned),
                    original,
                    path: Some(path.to_owned()),
                    frontmatter,
                });
            }
            offset += line.len();
        }
        anyhow::bail!("子智能体 YAML 头部缺少结束标记")
    }

    pub(crate) fn document(
        &self,
        name: &str,
        description: &str,
        prompt: &str,
        model: Option<&str>,
    ) -> anyhow::Result<String> {
        ensure!(!name.trim().is_empty(), "请填写子智能体名称");
        settings::native_name(name)?;
        ensure!(
            !["main", "sub"].contains(&name.to_lowercase().as_str()),
            "main 和 sub 是 OMP 保留名称"
        );
        ensure!(!description.trim().is_empty(), "请填写子智能体的职责说明");
        ensure!(!prompt.trim().is_empty(), "请填写系统提示词");
        let doc = Document::from_str(if self.frontmatter.is_empty() {
            "{}\n"
        } else {
            &self.frontmatter
        })?;
        let mut fields = vec![
            ("name", Some(Value::String(name.into()))),
            ("description", Some(Value::String(description.into()))),
        ];
        // Preserve advanced prioritized model arrays when the picker was not changed.
        if model != self.model.as_deref() {
            fields.push(("model", model.map(|s| Value::String(s.into()))));
        }
        for (key, value) in fields {
            if let Some(value) = value {
                let entry = Document::from_str(&format!("value: {}\n", value))?;
                doc.try_set_path(key, entry.get("value").context("无效的子智能体字段")?)?;
            } else if let Some(map) = doc.as_mapping() {
                map.remove(key);
            }
        }
        let newline = if prompt.ends_with('\n') { "" } else { "\n" };
        Ok(format!(
            "---\n{}\n---\n\n{prompt}{newline}",
            doc.to_string().trim_end()
        ))
    }

    pub(crate) fn save(&self, base: &Path, name: &str, text: &str) -> anyhow::Result<()> {
        settings::native_name(name)?;
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| base.join("agents").join(format!("{name}.md")));
        ensure!(
            self.path.is_some() || !path.exists(),
            "同名子智能体已存在，请打开后编辑"
        );
        settings::save_document(&path, &self.original, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn form_creates_native_agent_yaml_and_validates_required_fields() {
        let draft = AgentDraft::default();
        let text = draft
            .document(
                "reviewer",
                "检查：代码",
                "检查代码\n并给出结论",
                Some("@review"),
            )
            .unwrap();
        assert!(text.contains("model: \"@review\"") || text.contains("model: '@review'"));
        assert!(text.ends_with("检查代码\n并给出结论\n"));
        for name in ["", "../outside", "main", "SUB"] {
            assert!(draft.document(name, "role", "prompt", None).is_err());
        }
        assert!(draft.document("review", "", "prompt", None).is_err());
        assert!(draft.document("review", "role", "", None).is_err());
    }

    #[test]
    fn editing_preserves_advanced_fields_and_refuses_external_overwrites() {
        let root =
            std::env::temp_dir().join(format!("omp-gui-agent-form-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("agents")).unwrap();
        let path = root.join("agents/reviewer.md");
        std::fs::write(&path, "---\n# keep metadata\nname: reviewer\ndescription: Original\nmodel: ['@review', '@default']\ntools: [read, task]\nfuture: {enabled: true}\n---\n\nOriginal prompt\n").unwrap();
        let draft = AgentDraft::load(&path).unwrap();
        let text = draft
            .document("reviewer", "Updated", "New prompt\n", Some("@review"))
            .unwrap();
        assert!(text.contains("# keep metadata"));
        let metadata: Value = serde_yaml_ng::from_str(text.split("---").nth(1).unwrap()).unwrap();
        assert_eq!(
            metadata["model"],
            serde_json::json!(["@review", "@default"])
        );
        assert_eq!(metadata["tools"], serde_json::json!(["read", "task"]));
        assert_eq!(metadata["future"]["enabled"], true);
        draft.save(&root, "reviewer", &text).unwrap();
        let loaded = AgentDraft::load(&path).unwrap();
        assert_eq!(loaded.description, "Updated");
        assert_eq!(loaded.prompt, "New prompt\n");
        std::fs::write(&path, "external change").unwrap();
        assert!(loaded.save(&root, "reviewer", &text).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external change");
    }
}

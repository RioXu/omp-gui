//! Merge the shipped public directory with the installed OMP's effective registry.
//! Availability is decided by OMP; the GUI never reads or exports credential files.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default)]
pub(crate) struct ServiceCatalog {
    pub provider_ids: BTreeSet<String>,
    pub models: BTreeMap<String, Vec<Value>>,
    pub available: BTreeSet<String>,
    pub warning: Option<String>,
}
impl ServiceCatalog {
    pub fn load(omp: &str, cwd: &Path) -> Self {
        let bundled: Value =
            serde_json::from_str(include_str!("../../resources/omp-services.json"))
                .expect("validated public catalog");
        let mut result = Self {
            provider_ids: bundled["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            ..Self::default()
        };
        for m in bundled["models"].as_array().into_iter().flatten() {
            result.merge(m.clone());
        }
        match settings::command(omp, cwd, &["models", "ls", "--kind", "all", "--json"])
            .and_then(|s| Ok(serde_json::from_str::<Value>(&s)?))
        {
            Ok(native) => {
                for model in native["models"].as_array().into_iter().flatten() {
                    result.available.insert(selector(model));
                    result.merge(model.clone());
                }
            }
            Err(e) => {
                result.warning = Some(format!(
                    "无法读取本机可用模型：{e}。当前显示随 GUI 附带的目录。"
                ))
            }
        }
        result
    }
    fn merge(&mut self, model: Value) {
        let Some(provider) = model["provider"].as_str() else {
            return;
        };
        if model.get("kind").is_none_or(|k| k == "chat") {
            self.provider_ids.insert(provider.to_owned());
        }
        let models = self.models.entry(provider.to_owned()).or_default();
        if let Some(old) = models.iter_mut().find(|m| m["id"] == model["id"]) {
            if let (Some(old), Some(new)) = (old.as_object_mut(), model.as_object()) {
                old.extend(new.clone());
            }
        } else {
            models.push(model);
        }
    }
    pub fn search_options(&self) -> Vec<String> {
        self.models
            .values()
            .flatten()
            .filter(|m| m["kind"] == "search" || m["webSearch"].is_string())
            .map(selector)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn registered_providers(&self) -> BTreeSet<String> {
        self.models
            .iter()
            .filter(|(p, models)| {
                p.as_str() != "web" && models.iter().any(|m| self.available.contains(&selector(m)))
            })
            .map(|(p, _)| p.clone())
            .collect()
    }
    pub fn chat_providers(&self) -> Vec<String> {
        self.provider_ids
            .iter()
            .filter(|p| p.as_str() != "web")
            .cloned()
            .collect()
    }
}
pub(crate) fn selector(model: &Value) -> String {
    format!(
        "{}/{}",
        model["provider"].as_str().unwrap_or_default(),
        model["id"].as_str().unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "reads installed OMP registry"]
    fn installed_registry_includes_configured_deepseek() {
        let omp =
            std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
        let c = ServiceCatalog::load(omp.to_str().unwrap(), &std::env::current_dir().unwrap());
        assert!(c.warning.is_none(), "{:?}", c.warning);
        assert!(c.registered_providers().contains("deepseek"));
        assert!(c.available.iter().any(|s| s.starts_with("deepseek/")));
        assert!(c.search_options().len() >= 22);
        println!(
            "{} available models; {} providers in directory",
            c.available.len(),
            c.models.len()
        );
    }
    #[test]
    #[ignore = "validates an isolated models.yml against installed OMP"]
    fn installed_omp_accepts_minimal_native_model_override() {
        let root =
            std::env::temp_dir().join(format!("omp-gui-native-override-{}", crate::domain::id()));
        std::fs::create_dir_all(&root).unwrap();
        let store = Providers::load(&root).unwrap();
        store.save("deepseek",Some(&json!({"apiKey":"fixture-only-not-a-secret","modelOverrides":{"deepseek-flash":{"contextWindow":2000000}}}))).unwrap();
        let omp =
            std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("omp/omp.exe");
        let out = hidden_command(omp.to_str().unwrap())
            .env("PI_CODING_AGENT_DIR", &root)
            .current_dir(&root)
            .args(["models", "ls", "deepseek", "--json"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let output: Value = serde_json::from_slice(&out.stdout).unwrap();
        let model = output["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "deepseek-flash")
            .expect("native model survives override");
        assert_eq!(model["contextWindow"], 2000000);
    }
    #[test]
    fn native_models_override_metadata_without_losing_search_capability() {
        let mut c = ServiceCatalog::default();
        c.merge(
            json!({"provider":"deepseek","id":"test","webSearch":"deepseek","contextWindow":100}),
        );
        c.merge(json!({"provider":"deepseek","id":"test","contextWindow":200}));
        c.available.insert("deepseek/test".into());
        assert_eq!(c.models["deepseek"].len(), 1);
        assert_eq!(c.models["deepseek"][0]["contextWindow"], 200);
        assert_eq!(c.search_options(), vec!["deepseek/test"]);
        assert!(c.registered_providers().contains("deepseek"));
    }
    #[test]
    fn bundled_catalog_includes_every_native_search_engine() {
        let data: Value =
            serde_json::from_str(include_str!("../../resources/omp-services.json")).unwrap();
        let providers = data["providers"].as_array().unwrap();
        for id in ["ollama", "lm-studio", "vllm", "charm-hyper"] {
            assert!(
                providers.iter().any(|v| v == id),
                "missing dynamic provider {id}"
            );
        }
        let engines: BTreeSet<_> = data["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["provider"] == "web")
            .filter_map(|m| m["id"].as_str())
            .collect();
        for id in [
            "perplexity",
            "zai",
            "exa",
            "tinyfish",
            "jina",
            "kagi",
            "tavily",
            "firecrawl",
            "brave",
            "deepseek",
            "kimi",
            "parallel",
            "synthetic",
            "ollama",
            "searxng",
            "duckduckgo",
            "google",
            "ecosia",
            "startpage",
            "mojeek",
            "public",
        ] {
            assert!(engines.contains(id), "{id}");
        }
    }
}

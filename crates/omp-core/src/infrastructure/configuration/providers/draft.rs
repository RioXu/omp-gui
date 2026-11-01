//! Build minimal overrides; do not materialize the entire native model catalog into YAML.
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ProviderDraft {
    pub native_provider: bool,
    pub native_models: BTreeMap<String, Value>,
    pub original_url: String,
    pub original_api: String,
}
impl ProviderDraft {
    pub fn value(&self, original: &Value, models: &[Value], url: &str, api: &str) -> Value {
        let mut value = original.clone();
        if url != self.original_url || !self.native_provider {
            if url.is_empty() {
                value.as_object_mut().map(|v| v.remove("baseUrl"));
            } else {
                value["baseUrl"] = json!(url);
            }
        }
        if api != self.original_api || !self.native_provider {
            value["api"] = json!(api);
        }
        let custom = models
            .iter()
            .filter(|m| {
                !self
                    .native_models
                    .contains_key(m["id"].as_str().unwrap_or_default())
            })
            .cloned()
            .collect::<Vec<_>>();
        if !custom.is_empty() || value.get("models").is_some() {
            value["models"] = json!(custom);
        }
        for model in models {
            let id = model["id"].as_str().unwrap_or_default();
            if let Some(baseline) = self.native_models.get(id) {
                for field in ["name", "contextWindow", "maxTokens", "reasoning", "input"] {
                    if baseline[field] != model[field] {
                        value["modelOverrides"][id][field] = model[field].clone();
                    }
                }
            }
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_native_limits_preserves_unknown_overrides_and_endpoint() {
        let m = json!({"id":"test","contextWindow":100,"maxTokens":50});
        let d = ProviderDraft {
            native_provider: true,
            native_models: BTreeMap::from([("test".into(), m.clone())]),
            original_url: "https://native".into(),
            original_api: "native-api".into(),
        };
        let original = json!({"future":true,"modelOverrides":{"test":{"futureParameter":42}}});
        let mut changed = m;
        changed["contextWindow"] = json!(200);
        let v = d.value(&original, &[changed], "https://native", "native-api");
        assert!(v.get("models").is_none());
        assert!(v.get("baseUrl").is_none());
        assert!(v.get("api").is_none());
        assert_eq!(v["modelOverrides"]["test"]["contextWindow"], 200);
        assert_eq!(v["modelOverrides"]["test"]["futureParameter"], 42);
        assert_eq!(v["future"], true);
    }
}

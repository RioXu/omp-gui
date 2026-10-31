use crate::infrastructure::configuration::settings;
use anyhow::Context;
use serde_json::Value;
use std::path::Path;

pub(crate) fn available_models(omp: &str, cwd: &Path) -> anyhow::Result<Vec<Value>> {
    let output = settings::command(omp, cwd, &["models", "ls", "--kind", "all", "--json"])?;
    let value: Value = serde_json::from_str(&output)?;
    Ok(value["models"]
        .as_array()
        .context("OMP 返回了无效的模型目录")?
        .clone())
}

pub(crate) fn split_effort(pattern: &str) -> (&str, &str) {
    match pattern.rsplit_once(':') {
        Some((model, level))
            if [
                "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto", "inherit",
            ]
            .contains(&level) =>
        {
            (model, level)
        }
        _ => (pattern, ""),
    }
}

pub(crate) fn choices(models: &[Value], role: &str) -> Vec<String> {
    let mut items = models
        .iter()
        .filter(|m| {
            let kind = m["kind"].as_str().unwrap_or("chat");
            match role {
                "web" => kind == "search",
                "image" => kind == "image",
                "speech" => kind == "tts",
                "dictation" => kind == "stt",
                "judge" => ["judge", "tiny", "chat"].contains(&kind),
                "tiny" | "memory" => ["tiny", "chat"].contains(&kind),
                _ => kind == "chat",
            }
        })
        .filter_map(|m| Some(format!("{}/{}", m["provider"].as_str()?, m["id"].as_str()?)))
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

pub(crate) fn effort_options(models: &[Value], selector: &str) -> Vec<String> {
    let mut values = Vec::new();
    if let Some(model) = models.iter().find(|m| {
        format!(
            "{}/{}",
            m["provider"].as_str().unwrap_or(""),
            m["id"].as_str().unwrap_or("")
        ) == selector
    }) {
        if model["kind"].as_str().is_some_and(|kind| kind != "chat") {
            return values;
        }
        values.push("off".into());
        if model["reasoning"] == true {
            values.push("auto".into());
            for level in model["thinking"]
                .as_array()
                .or_else(|| model["thinking"]["efforts"].as_array())
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !values.iter().any(|v| v == level) {
                    values.push(level.into());
                }
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn capabilities_and_efforts_follow_native_model_metadata() {
        let models = vec![
            json!({"provider":"deepseek","id":"flash","kind":"chat","reasoning":true,"thinking":["low","high","max"]}),
            json!({"provider":"web","id":"search","kind":"search"}),
        ];
        assert_eq!(choices(&models, "default"), ["deepseek/flash"]);
        assert_eq!(choices(&models, "web"), ["web/search"]);
        assert_eq!(
            effort_options(&models, "deepseek/flash"),
            ["off", "auto", "low", "high", "max"]
        );
        assert!(effort_options(&models, "web/search").is_empty());
        assert_eq!(
            split_effort("provider/model:high"),
            ("provider/model", "high")
        );
        assert_eq!(
            split_effort("provider/model:custom"),
            ("provider/model:custom", "")
        );
    }
}

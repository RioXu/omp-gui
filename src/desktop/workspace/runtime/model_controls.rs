use super::*;
use std::collections::BTreeMap;

/// The native available registry already applies provider configuration and
/// credentials. Never populate the composer from the public service catalog.
pub(crate) fn model_groups(models: &[Value]) -> BTreeMap<String, Vec<Value>> {
    let mut groups = BTreeMap::<String, Vec<Value>>::new();
    for model in models {
        let (Some(provider), Some(id)) = (model["provider"].as_str(), model["id"].as_str()) else {
            continue;
        };
        if provider.is_empty()
            || id.is_empty()
            || model.get("kind").is_some_and(|kind| kind != "chat")
        {
            continue;
        }
        let entries = groups.entry(provider.into()).or_default();
        if !entries.iter().any(|m| m["id"] == id) {
            entries.push(model.clone());
        }
    }
    for models in groups.values_mut() {
        models.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    groups
}

pub(crate) fn model_key(model: &Value) -> String {
    format!(
        "{}/{}",
        model["provider"].as_str().unwrap_or_default(),
        model["id"].as_str().unwrap_or_default()
    )
}

fn fallback_levels(model: &Value) -> Vec<String> {
    let mut levels = vec!["off".into()];
    if model["reasoning"] == false {
        return levels;
    }
    if let Some(efforts) = model["thinking"]["efforts"]
        .as_array()
        .or_else(|| model["thinking"].as_array())
    {
        for value in efforts.iter().filter_map(Value::as_str) {
            if !levels.iter().any(|v| v == value) {
                levels.push(value.into());
            }
        }
    }
    levels
}

impl Workspace {
    pub(in crate::desktop::workspace) fn image_input_available(&self) -> bool {
        std::iter::once(&self.runtime.model_controls.current)
            .chain(self.runtime.model_controls.available.iter())
            .any(|model| {
                model["input"]
                    .as_array()
                    .is_some_and(|input| input.iter().any(|v| v == "image"))
            })
    }
    pub fn choose_model(&mut self, provider: &str, model: &str, cx: &mut Context<Self>) {
        if self.runtime.running
            || !self.runtime.connected
            || self.runtime.model_controls.pending.is_some()
        {
            return;
        }
        let request = id();
        self.runtime.model_controls.pending = Some((request.clone(), None));
        self.command(json!({"type":"set_model","id":request,"provider":provider,"modelId":model}));
        cx.notify();
    }

    pub fn choose_thinking(&mut self, level: &str, cx: &mut Context<Self>) {
        if self.runtime.running
            || !self.runtime.connected
            || self.runtime.model_controls.pending.is_some()
            || !self.thinking_options().iter().any(|v| v == level)
        {
            return;
        }
        let request = id();
        self.runtime.model_controls.pending = Some((request.clone(), Some(level.into())));
        // "auto" is a session selector, not a concrete ThinkingLevel accepted
        // by set_thinking_level. The native slash command enables its classifier.
        self.command(thinking_command(level, &request));
        cx.notify();
    }

    pub fn thinking_options(&self) -> Vec<String> {
        let mut levels = self.runtime.model_controls.levels.clone();
        if levels.iter().any(|l| l != "off") && !levels.iter().any(|l| l == "auto") {
            levels.insert(0, "auto".into());
        }
        levels
    }

    pub(in crate::desktop::workspace) fn request_thinking_levels(&mut self) {
        let request = id();
        self.runtime.model_controls.levels_request = Some(request.clone());
        self.runtime.model_controls.levels.clear();
        self.command(json!({"type":"get_available_thinking_levels","id":request}));
    }

    pub(in crate::desktop::workspace) fn request_thinking_source(&mut self) {
        let mut request = json!({"type":"get_entries","id":"thinking-source"});
        if let Some(cursor) = &self.runtime.model_controls.entries_cursor {
            request["since"] = json!(cursor);
        }
        self.command(request);
    }

    pub(in crate::desktop::workspace) fn on_model_control(&mut self, frame: &Value) -> bool {
        let request = frame["id"].as_str().unwrap_or_default();
        if self.runtime.model_controls.levels_request.as_deref() == Some(request) {
            self.runtime.model_controls.levels_request = None;
            self.runtime.model_controls.levels = if frame["success"] == true {
                frame["data"]["levels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            } else {
                fallback_levels(&self.runtime.model_controls.current)
            };
            return true;
        }
        if request == "thinking-source" {
            if let Some(entries) = frame["data"]["entries"]
                .as_array()
                .or_else(|| frame["data"].as_array())
            {
                for entry in entries {
                    if let Some(cursor) = entry["id"].as_str() {
                        self.runtime.model_controls.entries_cursor = Some(cursor.into());
                    }
                    if entry["type"] == "thinking_level_change" {
                        self.runtime.model_controls.auto = entry["configured"] == "auto";
                    }
                }
            }
            return true;
        }
        if self
            .runtime
            .model_controls
            .pending
            .as_ref()
            .is_none_or(|(id, _)| id != request)
        {
            return false;
        }
        let (_, level) = self.runtime.model_controls.pending.take().unwrap();
        if frame["success"] == false {
            self.runtime.error = Some(
                frame["error"]
                    .as_str()
                    .unwrap_or("无法切换模型或推理强度")
                    .into(),
            );
            return true;
        }
        if let Some(level) = level {
            self.runtime.model_controls.auto = level == "auto";
        } else {
            self.runtime.model_controls.current = frame["data"].clone();
            self.request_thinking_levels();
        }
        self.request_state();
        self.request_thinking_source();
        true
    }
}

fn thinking_command(level: &str, request: &str) -> Value {
    if level == "auto" {
        json!({"type":"prompt","id":request,"message":"/effort auto"})
    } else {
        json!({"type":"set_thinking_level","id":request,"level":level})
    }
}

#[cfg(test)]
mod tests {
    use super::{fallback_levels, model_groups, thinking_command};
    use serde_json::json;
    #[test]
    fn all_available_provider_models_are_grouped_without_public_catalog_entries() {
        let groups = model_groups(&[
            json!({"provider":"custom","id":"b"}),
            json!({"provider":"deepseek","id":"same"}),
            json!({"provider":"custom","id":"same"}),
            json!({"provider":"custom","id":"a"}),
            json!({"provider":"custom","id":"b"}),
            json!({"provider":"web","id":"search","kind":"search"}),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups["custom"]
                .iter()
                .map(|m| m["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["a", "b", "same"]
        );
        assert_eq!(groups["deepseek"].len(), 1);
    }
    #[test]
    fn native_efforts_are_preserved_and_auto_uses_the_session_selector() {
        let levels = fallback_levels(
            &json!({"reasoning":true,"thinking":{"mode":"effort","efforts":["low","high","max"]}}),
        );
        assert_eq!(levels, vec!["off", "low", "high", "max"]);
        assert!(!levels.iter().any(|l| l == "xhigh"));
        assert_eq!(fallback_levels(&json!({"reasoning":false})), vec!["off"]);
        assert_eq!(
            thinking_command("auto", "r"),
            json!({"type":"prompt","id":"r","message":"/effort auto"})
        );
        assert_eq!(
            thinking_command("max", "r"),
            json!({"type":"set_thinking_level","id":"r","level":"max"})
        );
    }
}

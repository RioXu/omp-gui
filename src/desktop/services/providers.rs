use super::*;

impl RegistryView {
    pub(super) fn edit_provider(
        &mut self,
        id: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.provider_draft.native_provider = id
            .as_ref()
            .is_some_and(|id| self.services.provider_ids.contains(id));
        self.draft = id
            .as_ref()
            .and_then(|id| self.providers.values.get(id))
            .cloned()
            .unwrap_or(json!({}));
        let (url, api) = match id.as_deref() {
            Some("ollama") => ("http://localhost:11434/v1", "openai-responses"),
            Some("lm-studio") => ("http://127.0.0.1:1234/v1", "openai-completions"),
            _ => ("", "openai-completions"),
        };
        self.provider_draft.native_models = id
            .as_ref()
            .and_then(|id| self.services.models.get(id))
            .into_iter()
            .flatten()
            .filter(|m| m.get("kind").is_none_or(|k| k == "chat"))
            .filter_map(|m| Some((m["id"].as_str()?.to_string(), m.clone())))
            .collect();
        let native = self.provider_draft.native_models.values().next();
        let url = native.and_then(|m| m["baseUrl"].as_str()).unwrap_or(url);
        let api = native.and_then(|m| m["api"].as_str()).unwrap_or(api);
        self.set("provider", id.clone().unwrap_or_default(), w, cx);
        self.set("url", self.draft["baseUrl"].as_str().unwrap_or(url), w, cx);
        self.set("key", "", w, cx);
        let format = self.draft["api"].as_str().unwrap_or(api).to_string();
        self.api.update(cx, |s, cx| {
            let mut formats = APIS.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            if !formats.contains(&format) {
                formats.push(format.clone());
            }
            s.set_items(formats, w, cx);
            s.set_selected_value(&format, w, cx);
        });
        self.provider_draft.original_url = self.value("url", cx);
        self.provider_draft.original_api = format;
        self.models = self
            .provider_draft
            .native_models
            .values()
            .cloned()
            .collect();
        for model in self.draft["models"].as_array().into_iter().flatten() {
            // Explicit custom rows keep their existing serialization and ownership.
            if let Some(id) = model["id"].as_str() {
                self.provider_draft.native_models.remove(id);
            }
            self.models.retain(|m| m["id"] != model["id"]);
            self.models.push(model.clone());
        }
        self.selected = id;
        self.editing = true;
        self.model_form = false;
        self.discovered.clear();
        self.delete_confirm = false;
        self.notice.clear();
        cx.notify();
    }
    pub(super) fn edit_model(
        &mut self,
        ix: Option<usize>,
        candidate: Option<String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = ix
            .and_then(|i| self.models.get(i))
            .cloned()
            .unwrap_or(json!({}));
        self.set(
            "model_id",
            candidate.unwrap_or_else(|| m["id"].as_str().unwrap_or("").to_string()),
            w,
            cx,
        );
        self.set("model_name", m["name"].as_str().unwrap_or(""), w, cx);
        self.set(
            "context",
            m["contextWindow"].as_u64().unwrap_or(128000).to_string(),
            w,
            cx,
        );
        self.set(
            "tokens",
            m["maxTokens"].as_u64().unwrap_or(16384).to_string(),
            w,
            cx,
        );
        self.reasoning = m["reasoning"].as_bool().unwrap_or(false);
        self.vision = m["input"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == "image"));
        self.model_edit = ix;
        self.model_form = true;
        cx.notify();
    }
    pub(super) fn save_model(&mut self, cx: &mut Context<Self>) {
        let id = self.value("model_id", cx);
        let context = self.value("context", cx).parse::<u64>();
        let tokens = self.value("tokens", cx).parse::<u64>();
        if id.is_empty()
            || !context.as_ref().is_ok_and(|v| *v > 0)
            || !tokens.as_ref().is_ok_and(|v| *v > 0)
        {
            self.notice = "请输入模型 ID、正整数上下文长度和输出上限".into();
            cx.notify();
            return;
        }
        if self
            .models
            .iter()
            .enumerate()
            .any(|(ix, m)| Some(ix) != self.model_edit && m["id"] == id)
        {
            self.notice = "此模型 ID 已存在".into();
            cx.notify();
            return;
        }
        let mut m = self
            .model_edit
            .and_then(|i| self.models.get(i))
            .cloned()
            .unwrap_or(json!({"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}));
        m["id"] = json!(id);
        let name = self.value("model_name", cx);
        m["name"] = if name.is_empty() {
            m["id"].clone()
        } else {
            json!(name)
        };
        m["contextWindow"] = json!(context.unwrap());
        m["maxTokens"] = json!(tokens.unwrap());
        m["reasoning"] = json!(self.reasoning);
        m["input"] = if self.vision {
            json!(["text", "image"])
        } else {
            json!(["text"])
        };
        if let Some(ix) = self.model_edit {
            self.models[ix] = m;
        } else {
            self.models.push(m);
        }
        self.model_form = false;
        self.notice = "模型已加入待保存配置".into();
        cx.notify();
    }
    pub(super) fn save_provider(&mut self, delete: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let id = self.value("provider", cx);
        let url = self.value("url", cx);
        if !delete
            && !url.is_empty()
            && reqwest::Url::parse(&url)
                .ok()
                .is_none_or(|u| !["https", "http"].contains(&u.scheme()))
        {
            self.notice = "请输入有效的 HTTP(S) API 地址".into();
            cx.notify();
            return;
        }
        let api = self
            .api
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(APIS[0].into());
        let mut value = self
            .provider_draft
            .value(&self.draft, &self.models, &url, &api);
        let key = self.value("key", cx);
        if !key.is_empty() {
            value["apiKey"] = json!(key);
        }
        let store = self.providers.clone();
        let root = self.root.clone();
        let tx = self.tx.clone();
        let configuration = self.configuration.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = configuration
                .call::<()>(ConfigAction::SaveProvider {
                    store: Box::new(store),
                    id,
                    value: (!delete).then_some(value),
                })
                .and_then(|_| configuration.providers(&root))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(ResultEvent::Saved(result));
        });
        cx.notify();
    }
    pub(super) fn discover(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let base = self.value("url", cx);
        let mut key = self.value("key", cx);
        if key.is_empty() {
            key = self.draft["apiKey"].as_str().unwrap_or("").into();
        }
        let api = self
            .api
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_default();
        let tx = self.tx.clone();
        let configuration = self.configuration.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Discovered(
                configuration
                    .call(ConfigAction::DiscoverModels { base, key, api })
                    .map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
}

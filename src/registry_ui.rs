use crate::{
    registry::{self, Providers},
    settings::Catalog,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    form::{field, v_form},
    input::{Input, InputState},
    select::{Select, SelectState},
    switch::Switch,
};
use gpui_kit::{prelude::FluentBuilder, *};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, time::Duration};

pub(crate) enum RegistryEvent {
    Saved,
    Close,
}
impl EventEmitter<RegistryEvent> for RegistryView {}
enum ResultEvent {
    Loaded(Result<Providers, String>),
    Saved(Result<Providers, String>),
    Discovered(Result<Vec<String>, String>),
    WebSaved(Result<(), String>),
}
pub(crate) struct RegistryView {
    omp: String,
    cwd: PathBuf,
    root: PathBuf,
    project: bool,
    web: bool,
    providers: Providers,
    selected: Option<String>,
    editing: bool,
    draft: Value,
    fields: BTreeMap<&'static str, Entity<InputState>>,
    api: Entity<SelectState<Vec<String>>>,
    search_service: Entity<SelectState<Vec<String>>>,
    models: Vec<Value>,
    model_edit: Option<usize>,
    model_form: bool,
    reasoning: bool,
    vision: bool,
    discovered: Vec<String>,
    model_picker: Entity<SelectState<Vec<String>>>,
    roles: Value,
    chains: Value,
    fallbacks: Vec<String>,
    web_enabled: bool,
    busy: bool,
    notice: String,
    delete_confirm: bool,
    tx: mpsc::Sender<ResultEvent>,
    rx: mpsc::Receiver<ResultEvent>,
}
const SERVICES: [&str; 11] = [
    "web/parallel",
    "web/exa",
    "web/firecrawl",
    "web/brave",
    "web/tavily",
    "web/searxng",
    "web/duckduckgo",
    "web/startpage",
    "web/google",
    "web/ecosia",
    "web/public",
];
const APIS: [&str; 4] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];
impl RegistryView {
    pub fn new(
        omp: String,
        cwd: PathBuf,
        catalog: &Catalog,
        project: bool,
        web: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut fields = BTreeMap::new();
        for name in [
            "provider",
            "url",
            "key",
            "model_id",
            "model_name",
            "context",
            "tokens",
            "search_key",
            "search_url",
            "search_token",
        ] {
            fields.insert(
                name,
                cx.new(|cx| {
                    InputState::new(w, cx)
                        .masked(["key", "search_key", "search_token"].contains(&name))
                }),
            );
        }
        let api = cx.new(|cx| {
            SelectState::new(
                APIS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                None,
                w,
                cx,
            )
        });
        let search_service = cx.new(|cx| {
            SelectState::new(
                SERVICES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                None,
                w,
                cx,
            )
        });
        api.update(cx, |s, cx| {
            s.set_selected_value(&APIS[0].to_string(), w, cx)
        });
        let layer = if project {
            &catalog.project
        } else {
            &catalog.global
        };
        let roles = layer
            .get("modelRoles")
            .and_then(|v| v.get("value"))
            .cloned()
            .unwrap_or(json!({}));
        let chains = layer
            .get("retry.fallbackChains")
            .and_then(|v| v.get("value"))
            .cloned()
            .unwrap_or(json!({}));
        let web_role = roles["web"]
            .as_str()
            .or_else(|| {
                catalog
                    .entries
                    .get("modelRoles")
                    .and_then(|v| v["value"]["web"].as_str())
            })
            .unwrap_or(SERVICES[0])
            .to_string();
        search_service.update(cx, |s, cx| {
            let mut services = SERVICES.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            if !services.contains(&web_role) {
                services.push(web_role.clone());
            }
            s.set_items(services, w, cx);
            s.set_selected_value(&web_role, w, cx);
        });
        let fallbacks = chains["web"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let web_enabled = catalog
            .entries
            .get("web_search.enabled")
            .and_then(|v| v["value"].as_bool())
            .unwrap_or(true);
        if let Some(url) = catalog
            .entries
            .get("searxng.endpoint")
            .and_then(|v| v["value"].as_str())
        {
            fields["search_url"].update(cx, |s, cx| s.set_value(url, w, cx));
        }
        let (tx, rx) = mpsc::channel();
        let root = catalog.root.clone();
        let mut this = Self {
            omp,
            cwd,
            root,
            project,
            web,
            providers: Providers::default(),
            selected: None,
            editing: false,
            draft: json!({}),
            fields,
            api,
            search_service,
            models: vec![],
            model_edit: None,
            model_form: false,
            reasoning: false,
            vision: false,
            discovered: vec![],
            model_picker: cx
                .new(|cx| SelectState::new(Vec::<String>::new(), None, w, cx).searchable(true)),
            roles,
            chains,
            fallbacks,
            web_enabled,
            busy: false,
            notice: String::new(),
            delete_confirm: false,
            tx,
            rx,
        };
        if !web {
            this.load();
        }
        cx.spawn_in(w, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        for result in this.rx.try_iter().collect::<Vec<_>>() {
                            this.busy = false;
                            match result {
                                ResultEvent::Loaded(Ok(v)) => {
                                    this.providers = v;
                                    this.notice.clear();
                                }
                                ResultEvent::Saved(Ok(v)) => {
                                    this.providers = v;
                                    this.editing = false;
                                    this.notice = "服务配置已保存".into();
                                    cx.emit(RegistryEvent::Saved);
                                }
                                ResultEvent::Discovered(Ok(ids)) => {
                                    this.model_picker
                                        .update(cx, |s, cx| s.set_items(ids.clone(), window, cx));
                                    this.discovered = ids;
                                    this.notice = "已获取服务提供的模型，可选择添加".into();
                                }
                                ResultEvent::WebSaved(Ok(())) => {
                                    this.notice = "搜索服务与当前范围的路由已保存".into();
                                    cx.emit(RegistryEvent::Saved);
                                }
                                ResultEvent::Loaded(Err(e))
                                | ResultEvent::Saved(Err(e))
                                | ResultEvent::Discovered(Err(e))
                                | ResultEvent::WebSaved(Err(e)) => this.notice = e,
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        this
    }
    fn load(&mut self) {
        self.busy = true;
        let root = self.root.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Loaded(
                Providers::load(&root).map_err(|e| format!("{e:#}")),
            ));
        });
    }
    fn value(&self, name: &str, cx: &App) -> String {
        self.fields[name].read(cx).value().trim().to_string()
    }
    fn set(&self, name: &str, value: impl Into<SharedString>, w: &mut Window, cx: &mut App) {
        self.fields[name].update(cx, |s, cx| s.set_value(value.into(), w, cx));
    }
    fn edit_provider(
        &mut self,
        id: Option<String>,
        preset: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft = id
            .as_ref()
            .and_then(|id| self.providers.values.get(id))
            .cloned()
            .unwrap_or(json!({}));
        let (name, url, api) = match preset {
            "OpenAI" => ("openai", "https://api.openai.com/v1", "openai-responses"),
            "Anthropic" => (
                "anthropic",
                "https://api.anthropic.com/v1",
                "anthropic-messages",
            ),
            "DeepSeek" => (
                "deepseek",
                "https://api.deepseek.com/v1",
                "openai-completions",
            ),
            "Ollama" => ("ollama", "http://localhost:11434/v1", "openai-completions"),
            _ => ("", "", "openai-completions"),
        };
        self.set("provider", id.clone().unwrap_or(name.into()), w, cx);
        self.set("url", self.draft["baseUrl"].as_str().unwrap_or(url), w, cx);
        self.set("key", "", w, cx);
        let format = self.draft["api"].as_str().unwrap_or(api).to_string();
        self.api
            .update(cx, |s, cx| s.set_selected_value(&format, w, cx));
        self.models = self.draft["models"].as_array().cloned().unwrap_or_default();
        self.selected = id;
        self.editing = true;
        self.model_form = false;
        self.discovered.clear();
        self.delete_confirm = false;
        self.notice.clear();
        cx.notify();
    }
    fn edit_model(
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
    fn save_model(&mut self, cx: &mut Context<Self>) {
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
    fn save_provider(&mut self, delete: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let id = self.value("provider", cx);
        let url = self.value("url", cx);
        if !delete
            && reqwest::Url::parse(&url)
                .ok()
                .is_none_or(|u| !["https", "http"].contains(&u.scheme()))
        {
            self.notice = "请输入有效的 HTTP(S) API 地址".into();
            cx.notify();
            return;
        }
        let mut value = self.draft.clone();
        value["baseUrl"] = json!(url);
        value["api"] = json!(
            self.api
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or(APIS[0].into())
        );
        value["models"] = json!(self.models);
        let key = self.value("key", cx);
        if !key.is_empty() {
            value["apiKey"] = json!(key);
        }
        let store = self.providers.clone();
        let root = self.root.clone();
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = store
                .save(&id, if delete { None } else { Some(&value) })
                .and_then(|_| Providers::load(&root))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(ResultEvent::Saved(result));
        });
        cx.notify();
    }
    fn discover(&mut self, cx: &mut Context<Self>) {
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
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Discovered(
                registry::discover_models(&base, &key, &api).map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
    fn save_web(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let service = self
            .search_service
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(SERVICES[0].into());
        let mut roles = self.roles.clone();
        roles["web"] = json!(service);
        let mut chains = self.chains.clone();
        chains["web"] = json!(self.fallbacks);
        let key = self.value("search_key", cx);
        let url = self.value("search_url", cx);
        let token = self.value("search_token", cx);
        if service == "web/searxng"
            && reqwest::Url::parse(&url)
                .ok()
                .is_none_or(|u| !["http", "https"].contains(&u.scheme()))
        {
            self.notice = "SearXNG 需要有效的服务地址".into();
            cx.notify();
            return;
        }
        let omp = self.omp.clone();
        let cwd = self.cwd.clone();
        let root = self.root.clone();
        let project = self.project;
        let enabled = self.web_enabled;
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                if !key.is_empty() {
                    registry::import_key(&omp, &cwd, service.trim_start_matches("web/"), &key)?;
                }
                if service == "web/searxng" {
                    registry::save_scoped(
                        &omp,
                        &cwd,
                        &root,
                        project,
                        "searxng.endpoint",
                        json!(url),
                    )?;
                    if !token.is_empty() {
                        registry::save_scoped(
                            &omp,
                            &cwd,
                            &root,
                            project,
                            "searxng.token",
                            json!(token),
                        )?;
                    }
                }
                registry::save_scoped(&omp, &cwd, &root, project, "modelRoles", roles)?;
                registry::save_scoped(&omp, &cwd, &root, project, "retry.fallbackChains", chains)?;
                registry::save_scoped(
                    &omp,
                    &cwd,
                    &root,
                    project,
                    "web_search.enabled",
                    json!(enabled),
                )
            })();
            let _ = tx.send(ResultEvent::WebSaved(result.map_err(|e| format!("{e:#}"))));
        });
        cx.notify();
    }
    fn model_form(&self, cx: &mut Context<Self>) -> AnyElement {
        v_form()
            .child(
                field()
                    .label("模型 ID")
                    .child(Input::new(&self.fields["model_id"])),
            )
            .child(
                field()
                    .label("显示名称")
                    .child(Input::new(&self.fields["model_name"])),
            )
            .child(
                field()
                    .label("上下文长度 · tokens")
                    .child(Input::new(&self.fields["context"])),
            )
            .child(
                field()
                    .label("最大输出 · tokens")
                    .child(Input::new(&self.fields["tokens"])),
            )
            .child(
                field().label("推理模型").child(
                    Switch::new("model-reasoning")
                        .checked(self.reasoning)
                        .on_click(cx.listener(|this, v: &bool, _, cx| {
                            this.reasoning = *v;
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().label("支持图片输入").child(
                    Switch::new("model-vision")
                        .checked(self.vision)
                        .on_click(cx.listener(|this, v: &bool, _, cx| {
                            this.vision = *v;
                            cx.notify();
                        })),
                ),
            )
            .child(
                field().child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("apply-model")
                                .primary()
                                .label("保留模型配置")
                                .on_click(cx.listener(|this, _, _, cx| this.save_model(cx))),
                        )
                        .child(Button::new("cancel-model").ghost().label("取消").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.model_form = false;
                                cx.notify();
                            }),
                        )),
                ),
            )
            .into_any_element()
    }
}
impl Render for RegistryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut body = div()
            .id("registry-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if self.web {
            body=body.child(v_form()
                .child(field().label("启用网络搜索").child(Switch::new("enable-web").checked(self.web_enabled).on_click(cx.listener(|this,v:&bool,_,cx|{this.web_enabled= *v;cx.notify();}))))
                .child(field().label("首选搜索服务").child(Select::new(&self.search_service)))
                .child(field().label("API 密钥 · 全局凭据").description("留空沿用 OMP 已有凭据；新密钥通过 OMP 原生凭据库保存。无需密钥的服务可留空。").child(Input::new(&self.fields["search_key"]).mask_toggle()))
                .child(field().label("SearXNG 地址").description("仅选择 SearXNG 时使用。").child(Input::new(&self.fields["search_url"])))
                .child(field().label("SearXNG Token").description("留空保留；按当前全局／项目范围保存。").child(Input::new(&self.fields["search_token"]).mask_toggle())));
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child("备用搜索服务 · 按顺序尝试"))
                    .child(
                        Button::new("add-search-fallback")
                            .small()
                            .label("加入所选服务")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(v) =
                                    this.search_service.read(cx).selected_value().cloned()
                                    && !this.fallbacks.contains(&v)
                                {
                                    this.fallbacks.push(v);
                                }
                                cx.notify();
                            })),
                    ),
            );
            for (ix, name) in self.fallbacks.iter().enumerate() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(name.clone()))
                        .child(
                            Button::new(SharedString::from(format!("web-up-{ix}")))
                                .ghost()
                                .small()
                                .label("上移")
                                .disabled(ix == 0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.fallbacks.swap(ix, ix - 1);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("web-remove-{ix}")))
                                .ghost()
                                .small()
                                .label("移除")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.fallbacks.remove(ix);
                                    cx.notify();
                                })),
                        ),
                );
            }
            body = body.child(
                Button::new("save-web")
                    .primary()
                    .label("保存搜索配置")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.save_web(cx))),
            );
        } else if !self.editing {
            body = body.child(div().text_sm().child(
                "选择服务，填写连接信息，再添加模型。已有 OMP 内置服务与账号仍由 OMP 管理。",
            ));
            let mut presets = div().flex().flex_wrap().gap_2();
            for preset in ["OpenAI", "Anthropic", "DeepSeek", "Ollama", "自定义"] {
                presets = presets.child(
                    Button::new(preset)
                        .label(preset)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.edit_provider(None, preset, w, cx)
                        })),
                );
            }
            body = body
                .child(presets)
                .child(div().mt_3().child("已注册的提供商"));
            if self.providers.values.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("尚未添加自定义提供商；可从上方开始注册。"),
                );
            }
            for (name, provider) in &self.providers.values {
                let id = name.clone();
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .border_1()
                        .border_color(theme.border)
                        .rounded_md()
                        .child(
                            div().flex_1().min_w_0().child(name.clone()).child(
                                div()
                                    .text_xs()
                                    .truncate()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        provider["baseUrl"]
                                            .as_str()
                                            .unwrap_or("使用内置端点")
                                            .to_string(),
                                    ),
                            ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("provider-{name}")))
                                .small()
                                .label("管理")
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.edit_provider(Some(id.clone()), "", w, cx)
                                })),
                        ),
                );
            }
        } else if self.model_form {
            body = body.child(self.model_form(cx));
        } else {
            body = body.child(
                v_form()
                    .child(
                        field()
                            .label("服务标识")
                            .description("OMP 使用此标识关联模型和角色。")
                            .child(
                                Input::new(&self.fields["provider"])
                                    .disabled(self.selected.is_some()),
                            ),
                    )
                    .child(
                        field()
                            .label("API 根地址")
                            .child(Input::new(&self.fields["url"])),
                    )
                    .child(field().label("API 格式").child(Select::new(&self.api)))
                    .child(
                        field()
                            .label("API 密钥或环境变量名称")
                            .description(if self.draft.get("apiKey").is_some() {
                                "已配置，留空保留现有凭据。"
                            } else {
                                "本地免密服务可留空；也可填写 OMP 可读取的环境变量名称。"
                            })
                            .child(Input::new(&self.fields["key"]).mask_toggle()),
                    ),
            );
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("discover-models")
                            .label("获取服务模型")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
                    )
                    .child(Button::new("add-model").label("手动添加模型").on_click(
                        cx.listener(|this, _, w, cx| this.edit_model(None, None, w, cx)),
                    )),
            );
            if !self.discovered.is_empty() {
                body = body
                    .child(
                        v_form().child(
                            field()
                                .label("服务提供的模型")
                                .child(Select::new(&self.model_picker)),
                        ),
                    )
                    .child(
                        Button::new("add-discovered-model")
                            .label("添加所选模型")
                            .on_click(cx.listener(|this, _, w, cx| {
                                if let Some(id) =
                                    this.model_picker.read(cx).selected_value().cloned()
                                {
                                    this.edit_model(None, Some(id), w, cx);
                                } else {
                                    this.notice = "请先在列表中选择模型".into();
                                    cx.notify();
                                }
                            })),
                    );
            }
            body = body.child(div().mt_2().child("已选择的模型"));
            for (ix, model) in self.models.iter().enumerate() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .py_2()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(model["id"].as_str().unwrap_or("").to_string())
                                .child(div().text_xs().text_color(theme.muted_foreground).child(
                                    format!(
                                        "上下文 {} · 输出 {}",
                                        model["contextWindow"], model["maxTokens"]
                                    ),
                                )),
                        )
                        .child(
                            Button::new(SharedString::from(format!("model-edit-{ix}")))
                                .ghost()
                                .small()
                                .label("参数")
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.edit_model(Some(ix), None, w, cx)
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("model-delete-{ix}")))
                                .ghost()
                                .small()
                                .label("移除")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.models.remove(ix);
                                    cx.notify();
                                })),
                        ),
                );
            }
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .mt_3()
                    .child(
                        Button::new("save-provider")
                            .primary()
                            .label("保存服务与模型")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save_provider(false, cx))),
                    )
                    .child(
                        Button::new("cancel-provider")
                            .ghost()
                            .label("取消")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editing = false;
                                cx.notify();
                            })),
                    )
                    .when(self.selected.is_some(), |d| {
                        d.child(
                            Button::new("delete-provider")
                                .ghost()
                                .label(if self.delete_confirm {
                                    "确认移除该服务配置"
                                } else {
                                    "移除服务"
                                })
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.delete_confirm {
                                        this.save_provider(true, cx);
                                    } else {
                                        this.delete_confirm = true;
                                        cx.notify();
                                    }
                                })),
                        )
                    }),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("back-registry")
                            .ghost()
                            .label("返回设置")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(RegistryEvent::Close))),
                    )
                    .child(div().text_lg().child(if self.web {
                        "网络搜索服务"
                    } else {
                        "模型服务与注册"
                    })),
            )
            .child(body)
            .child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(if self.busy {
                        "正在处理…".to_string()
                    } else {
                        self.notice.clone()
                    }),
            )
    }
}

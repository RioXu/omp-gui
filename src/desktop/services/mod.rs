mod directory;
mod model_form;
mod providers;
mod search;
mod view;

use crate::infrastructure::configuration::{
    providers::{self as registry, Providers},
    settings::Catalog,
};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    form::{field, v_form},
    input::{Input, InputState},
    select::{Select, SelectState},
    switch::Switch,
};
use gpui_kit::{prelude::FluentBuilder, *};
use omp_core::host::{ConfigAction, Configuration};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, time::Duration};

pub(crate) enum RegistryEvent {
    Saved,
    Close,
}
impl EventEmitter<RegistryEvent> for RegistryView {}
enum ResultEvent {
    Catalog(Box<registry::ServiceCatalog>),
    Loaded(Result<Providers, String>),
    Saved(Result<Providers, String>),
    Discovered(Result<Vec<String>, String>),
    WebSaved(Result<(), String>),
}
pub(crate) struct RegistryView {
    models_scroll: UniformListScrollHandle,
    search_scroll: UniformListScrollHandle,
    configuration: Configuration,
    root: PathBuf,
    project: bool,
    web: bool,
    providers: Providers,
    services: registry::ServiceCatalog,
    provider_picker: Entity<SelectState<Vec<String>>>,
    provider_draft: registry::ProviderDraft,
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
    catalog_loading: bool,
    notice: String,
    delete_confirm: bool,
    tx: mpsc::Sender<ResultEvent>,
    rx: mpsc::Receiver<ResultEvent>,
}
const DEFAULT_SEARCH: &str = "web/parallel";
const APIS: [&str; 4] = [
    "openai-completions",
    "openai-responses",
    "anthropic-messages",
    "google-generative-ai",
];
impl RegistryView {
    pub fn new(
        configuration: Configuration,
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
        let search_service =
            cx.new(|cx| SelectState::new(Vec::<String>::new(), None, w, cx).searchable(true));
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
            .unwrap_or(DEFAULT_SEARCH)
            .to_string();
        search_service.update(cx, |s, cx| {
            let mut services = Vec::new();
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
            models_scroll: UniformListScrollHandle::new(),
            search_scroll: UniformListScrollHandle::new(),
            configuration,
            root,
            project,
            web,
            providers: Providers::default(),
            services: registry::ServiceCatalog::default(),
            provider_picker: cx
                .new(|cx| SelectState::new(Vec::<String>::new(), None, w, cx).searchable(true)),
            provider_draft: registry::ProviderDraft::default(),
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
            catalog_loading: true,
            notice: String::new(),
            delete_confirm: false,
            tx,
            rx,
        };
        this.load();
        this.load_catalog();
        cx.spawn_in(w, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        for result in this.rx.try_iter().collect::<Vec<_>>() {
                            if !matches!(result, ResultEvent::Catalog(_)) {
                                this.busy = false;
                            }
                            match result {
                                ResultEvent::Catalog(catalog) => {
                                    this.catalog_loading = false;
                                    if let Some(warning) = &catalog.warning {
                                        this.notice = warning.clone();
                                    }
                                    let selected =
                                        this.search_service.read(cx).selected_value().cloned();
                                    let mut options = catalog.search_options();
                                    if let Some(v) = &selected
                                        && !options.contains(v)
                                    {
                                        options.push(v.clone());
                                    }
                                    this.search_service.update(cx, |s, cx| {
                                        s.set_items(options, window, cx);
                                        if let Some(v) = selected {
                                            s.set_selected_value(&v, window, cx);
                                        }
                                    });
                                    this.provider_picker.update(cx, |s, cx| {
                                        s.set_items(catalog.chat_providers(), window, cx)
                                    });
                                    this.services = *catalog;
                                }
                                ResultEvent::Loaded(Ok(v)) => {
                                    this.providers = v;
                                    this.notice.clear();
                                }
                                ResultEvent::Saved(Ok(v)) => {
                                    this.providers = v;
                                    this.editing = false;
                                    this.notice = "服务配置已保存".into();
                                    cx.emit(RegistryEvent::Saved);
                                    this.load_catalog();
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
                                    this.load_catalog();
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
        let configuration = self.configuration.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Loaded(
                configuration.providers(&root).map_err(|e| format!("{e:#}")),
            ));
        });
    }
    fn load_catalog(&mut self) {
        self.catalog_loading = true;
        let configuration = self.configuration.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Catalog(Box::new(
                configuration
                    .services()
                    .unwrap_or_else(|error| registry::ServiceCatalog {
                        warning: Some(format!("{error:#}")),
                        ..Default::default()
                    }),
            )));
        });
    }
    fn value(&self, name: &str, cx: &App) -> String {
        self.fields[name].read(cx).value().trim().to_string()
    }
    fn set(&self, name: &str, value: impl Into<SharedString>, w: &mut Window, cx: &mut App) {
        self.fields[name].update(cx, |s, cx| s.set_value(value.into(), w, cx));
    }
}

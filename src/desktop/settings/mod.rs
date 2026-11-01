mod authoring;
mod controls;
mod editing;
mod editors;
mod navigation;
mod sections;
mod view;
pub(crate) use navigation::Destination;

use crate::infrastructure::configuration::settings::{self, Catalog};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    select::{Select, SelectEvent, SelectState},
    sidebar::{SidebarItem, SidebarMenu, SidebarMenuItem},
    switch::Switch,
};
use gpui_kit::{prelude::FluentBuilder, *};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

pub(crate) enum SettingsEvent {
    OmpPath(String),
    Saved,
    Reconnect,
}
impl EventEmitter<SettingsEvent> for SettingsView {}
enum ResultEvent {
    Executable(String, Result<String, String>),
    Catalog(Result<Catalog, String>),
    Saved(Result<(), String>),
    Plugins(Result<String, String>),
    Inline(String, Option<Value>, bool, Result<(), String>),
}
enum Edit {
    Key { name: String, kind: String },
    File { path: PathBuf, original: String },
}
pub(crate) struct SettingsView {
    rows_scroll: UniformListScrollHandle,
    pending_destination: Option<Destination>,
    processing_speed: bool,
    registry: Option<Entity<crate::desktop::services::RegistryView>>,
    agent_editor: Option<Entity<editors::AgentEditor>>,
    role_editor: Option<Entity<editors::RoleEditor>>,
    omp: String,
    cwd: PathBuf,
    catalog: Catalog,
    project: bool,
    project_path: Option<PathBuf>,
    category: &'static str,
    search: Entity<InputState>,
    executable: Entity<InputState>,
    resolved_executable: String,
    name: Entity<InputState>,
    editor: Entity<TextareaState>,
    scalar: Entity<InputState>,
    edit: Option<Edit>,
    busy: bool,
    loading: bool,
    pending: HashSet<String>,
    pending_values: HashMap<String, Value>,
    enums: HashMap<String, Entity<SelectState<Vec<String>>>>,
    notice: String,
    plugins: String,
    tx: mpsc::Sender<ResultEvent>,
    rx: mpsc::Receiver<ResultEvent>,
    _subscriptions: Vec<Subscription>,
}
const CATEGORIES: [&str; 10] = [
    "常规",
    "模型",
    "网络搜索",
    "角色与提示词",
    "工具调用",
    "LSP",
    "技能",
    "MCP",
    "插件",
    "更多设置",
];
impl SettingsView {
    pub(crate) fn navigate(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.processing_speed = destination == Destination::ProcessingSpeed;
        let (category, query) = match destination {
            Destination::Page(category, query) => (category, query),
            Destination::ModelServices | Destination::ProcessingSpeed => ("模型", ""),
        };
        self.category = category;
        self.registry = None;
        self.search.update(cx, |input, cx| {
            input.set_value(query.to_owned(), window, cx)
        });
        self.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.prepare_controls(window, cx);
        if destination == Destination::ModelServices {
            self.project = false;
            if self.catalog.root.as_os_str().is_empty() {
                self.pending_destination = Some(destination);
            } else {
                self.pending_destination = None;
                self.open_registry(false, window, cx);
            }
        }
        cx.notify();
    }
    fn open_registry(&mut self, web: bool, w: &mut Window, cx: &mut Context<Self>) {
        let registry = cx.new(|cx| {
            crate::desktop::services::RegistryView::new(
                self.omp.clone(),
                self.cwd.clone(),
                &self.catalog,
                self.project,
                web,
                w,
                cx,
            )
        });
        self._subscriptions
            .push(cx.subscribe(&registry, |this, _, event, cx| {
                match event {
                    crate::desktop::services::RegistryEvent::Close => {
                        this.registry = None;
                        this.reload();
                    }
                    crate::desktop::services::RegistryEvent::Saved => {
                        this.reload();
                        cx.emit(SettingsEvent::Reconnect);
                    }
                }
                cx.notify();
            }));
        self.registry = Some(registry);
        cx.notify();
    }
    pub fn new(
        omp: String,
        cwd: PathBuf,
        project_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索设置名称或说明…"));
        let executable = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("自动：从当前环境 PATH 查找 omp；可填写绝对路径覆盖")
                .default_value(if omp == "omp" {
                    String::new()
                } else {
                    omp.clone()
                })
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("名称 / 插件包 / 本地路径"));
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(8, 22));
        let subscription = cx.subscribe_in(&search, window, |this, _, event, w, cx| {
            if matches!(event, InputEvent::Change) {
                this.prepare_controls(w, cx);
                cx.notify()
            }
        });
        let cached = settings::cached(&omp, &cwd).unwrap_or_default();
        let mut this = Self {
            rows_scroll: UniformListScrollHandle::new(),
            pending_destination: None,
            processing_speed: false,
            registry: None,
            agent_editor: None,
            role_editor: None,
            omp,
            cwd,
            catalog: cached,
            project: false,
            project_path,
            category: "常规",
            search,
            executable,
            resolved_executable: "正在检测…".into(),
            name,
            editor,
            scalar: cx.new(|cx| InputState::new(window, cx)),
            edit: None,
            busy: false,
            loading: false,
            pending: HashSet::new(),
            pending_values: HashMap::new(),
            enums: HashMap::new(),
            notice: String::new(),
            plugins: String::new(),
            tx,
            rx,
            _subscriptions: vec![subscription],
        };
        this.prepare_controls(window, cx);
        this.reload();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        for result in this.rx.try_iter().collect::<Vec<_>>() {
                            if matches!(result, ResultEvent::Saved(_) | ResultEvent::Plugins(_)) {
                                this.busy = false;
                            }
                            match result {
                                ResultEvent::Executable(command, result) => {
                                    if command == this.omp {
                                        this.resolved_executable =
                                            result.unwrap_or_else(|error| error);
                                    }
                                }
                                ResultEvent::Catalog(Ok(c)) => {
                                    this.loading = false;
                                    this.catalog = c;
                                    this.prepare_controls(window, cx);
                                    this.notice = "已读取本机 OMP 设置".into();
                                    if let Some(destination) = this.pending_destination.take() {
                                        this.navigate(destination, window, cx);
                                    }
                                }
                                ResultEvent::Saved(Ok(())) => {
                                    this.notice =
                                        "已保存。新建连接后完整生效；未中断正在运行的任务。".into();
                                    this.edit = None;
                                    cx.emit(SettingsEvent::Saved);
                                    this.reload();
                                }
                                ResultEvent::Plugins(Ok(s)) => {
                                    this.plugins = s;
                                    this.notice = "插件操作完成".into();
                                    cx.emit(SettingsEvent::Saved);
                                }
                                ResultEvent::Inline(key, value, project, result) => {
                                    this.pending.remove(&key);
                                    this.pending_values.remove(&key);
                                    match result {
                                        Ok(()) => {
                                            let layer = if project {
                                                &mut this.catalog.project
                                            } else {
                                                &mut this.catalog.global
                                            };
                                            if let Some(v) = value {
                                                layer.insert(
                                                    key.clone(),
                                                    serde_json::json!({"value":v}),
                                                );
                                            } else {
                                                layer.remove(&key);
                                            }
                                            this.notice = "已保存".into();
                                            cx.emit(SettingsEvent::Saved);
                                        }
                                        Err(e) => this.notice = e,
                                    }
                                    this.prepare_controls(window, cx);
                                    this.reload();
                                }
                                ResultEvent::Catalog(Err(e)) => {
                                    this.loading = false;
                                    this.notice = e;
                                }
                                ResultEvent::Saved(Err(e)) | ResultEvent::Plugins(Err(e)) => {
                                    this.notice = e
                                }
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
}

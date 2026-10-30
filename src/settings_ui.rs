mod controls;
mod editing;
mod sections;
mod view;

use crate::settings::{self, Catalog};
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
    registry: Option<Entity<crate::registry_ui::RegistryView>>,
    omp: String,
    cwd: PathBuf,
    catalog: Catalog,
    project: bool,
    project_path: Option<PathBuf>,
    category: &'static str,
    search: Entity<InputState>,
    executable: Entity<InputState>,
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
    fn open_registry(&mut self, web: bool, w: &mut Window, cx: &mut Context<Self>) {
        let registry = cx.new(|cx| {
            crate::registry_ui::RegistryView::new(
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
                    crate::registry_ui::RegistryEvent::Close => {
                        this.registry = None;
                        this.reload();
                    }
                    crate::registry_ui::RegistryEvent::Saved => {
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
        let executable = cx.new(|cx| InputState::new(window, cx).default_value(omp.clone()));
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
            registry: None,
            omp,
            cwd,
            catalog: cached,
            project: false,
            project_path,
            category: "常规",
            search,
            executable,
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
                                ResultEvent::Catalog(Ok(c)) => {
                                    this.loading = false;
                                    this.catalog = c;
                                    this.prepare_controls(window, cx);
                                    this.notice = "已读取本机 OMP 设置".into();
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

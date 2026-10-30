use crate::settings::{self, Catalog};
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
    registry: Option<Entity<crate::registry_ui::RegistryView>>,
    omp: String,
    cwd: PathBuf,
    catalog: Catalog,
    project: bool,
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
    fn prepare_controls(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        for (key, meta) in &self.catalog.entries {
            if meta["type"] != "enum" {
                continue;
            }
            let query = self.search.read(cx).value().to_lowercase();
            if settings::category(key) != self.category
                && (query.is_empty() || !key.to_lowercase().contains(&query))
            {
                continue;
            }
            let mut values = meta["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            let selected = layer
                .get(key)
                .and_then(|v| v["value"].as_str())
                .or_else(|| meta["value"].as_str())
                .map(str::to_owned);
            if let Some(value) = &selected
                && !values.contains(value)
            {
                values.push(value.clone());
            }
            if let Some(state) = self.enums.get(key) {
                state.update(cx, |state, cx| {
                    state.set_items(values, w, cx);
                    if let Some(v) = &selected {
                        state.set_selected_value(v, w, cx);
                    }
                });
            } else {
                let state = cx.new(|cx| SelectState::new(values, None, w, cx));
                state.update(cx, |s, cx| {
                    if let Some(v) = &selected {
                        s.set_selected_value(v, w, cx);
                    }
                });
                let name = key.clone();
                self._subscriptions
                    .push(cx.subscribe(&state, move |this, _, e, cx| {
                        if let SelectEvent::Confirm(Some(v)) = e {
                            this.inline_save(name.clone(), Some(Value::String(v.clone())), cx);
                        }
                    }));
                self.enums.insert(key.clone(), state);
            }
        }
    }
    fn inline_save(&mut self, key: String, value: Option<Value>, cx: &mut Context<Self>) {
        if self.pending.contains(&key) {
            return;
        }
        self.pending.insert(key.clone());
        if let Some(value) = &value {
            self.pending_values.insert(key.clone(), value.clone());
        }
        let tx = self.tx.clone();
        let project = self.project;
        let omp = self.omp.clone();
        let cwd = self.cwd.clone();
        let path = settings::config_path(&self.base());
        std::thread::spawn(move || {
            let result = if project {
                settings::set_project(&path, &key, value.clone())
            } else {
                match &value {
                    Some(v) => settings::command(
                        &omp,
                        &cwd,
                        &["config", "set", &key, &v.to_string(), "--json"],
                    )
                    .map(|_| ()),
                    None => settings::command(&omp, &cwd, &["config", "reset", &key, "--json"])
                        .map(|_| ()),
                }
            };
            let _ = tx.send(ResultEvent::Inline(
                key,
                value,
                project,
                result.map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
    fn setting_row(&self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let meta = &self.catalog.entries[key];
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        let configured = layer.get(key);
        let value = self.pending_values.get(key).or_else(|| {
            configured
                .and_then(|v| v.get("value"))
                .or_else(|| meta.get("value"))
        });
        let busy = self.pending.contains(key);
        let name = key.to_string();
        let control = if meta["type"] == "boolean" {
            Switch::new(SharedString::from(format!("toggle-{key}")))
                .checked(value.and_then(Value::as_bool).unwrap_or(false))
                .disabled(busy)
                .on_click(cx.listener(move |this, v: &bool, _, cx| {
                    this.inline_save(name.clone(), Some(Value::Bool(*v)), cx)
                }))
                .into_any_element()
        } else if let Some(state) = self.enums.get(key) {
            div()
                .w(rems(13.))
                .h(px(32.))
                .flex_shrink_0()
                .child(Select::new(state).w_full().disabled(busy))
                .into_any_element()
        } else {
            Button::new(SharedString::from(format!("edit-{key}")))
                .ghost()
                .small()
                .label("编辑")
                .accessibility_label(format!("编辑 {key}"))
                .disabled(busy)
                .on_click(cx.listener(move |this, _, w, cx| this.key(name.clone(), w, cx)))
                .into_any_element()
        };
        let reset = key.to_string();
        let description = meta["description"].as_str().unwrap_or("").to_owned();
        div()
            .id(SharedString::from(format!("setting-row-{key}")))
            .w_full()
            .h(px(112.))
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().truncate().child(key.to_string()))
                    .child(
                        div()
                            .id("description")
                            .text_xs()
                            .h(px(34.))
                            .overflow_hidden()
                            .text_color(cx.theme().muted_foreground)
                            .tooltip(move |w, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(description.clone())
                                    .build(w, cx)
                            })
                            .child(meta["description"].as_str().unwrap_or("").to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if configured.is_some() {
                                "此范围已配置"
                            } else {
                                "继承 / 默认 / 环境"
                            }),
                    ),
            )
            .child(
                div()
                    .w(rems(13.))
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(control),
            )
            .child(
                Button::new(SharedString::from(format!("reset-{key}")))
                    .ghost()
                    .xsmall()
                    .icon(IconName::RotateCw)
                    .tooltip("重置此范围")
                    .disabled(configured.is_none() || busy)
                    .on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.inline_save(reset.clone(), None, cx)
                        }),
                    ),
            )
            .into_any_element()
    }
    pub fn new(omp: String, cwd: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            registry: None,
            omp,
            cwd,
            catalog: cached,
            project: false,
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
                            this.busy = false;
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
                                ResultEvent::Catalog(Err(e))
                                | ResultEvent::Saved(Err(e))
                                | ResultEvent::Plugins(Err(e)) => this.notice = e,
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
    fn base(&self) -> PathBuf {
        if self.project {
            self.cwd.join(".omp")
        } else {
            self.catalog.root.clone()
        }
    }
    fn reload(&mut self) {
        self.loading = true;
        let tx = self.tx.clone();
        let omp = self.omp.clone();
        let cwd = self.cwd.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ResultEvent::Catalog(
                settings::catalog(&omp, &cwd).map_err(|e| format!("{e:#}")),
            ));
        });
    }
    fn key(&mut self, key: String, w: &mut Window, cx: &mut Context<Self>) {
        let Some(meta) = self.catalog.entries.get(&key) else {
            return;
        };
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        let value = layer
            .get(&key)
            .and_then(|v| v.get("value"))
            .or_else(|| meta.get("value"));
        let kind = meta["type"].as_str().unwrap_or("string").to_string();
        let text = value
            .map(|v| {
                if v.is_string() {
                    v.as_str().unwrap().to_string()
                } else {
                    serde_json::to_string_pretty(v).unwrap_or_default()
                }
            })
            .unwrap_or_default();
        let text = if kind == "record" && !layer.contains_key(&key) {
            "{}".to_owned()
        } else {
            text
        };
        self.scalar
            .update(cx, |i, cx| i.set_value(text.clone(), w, cx));
        self.editor.update(cx, |i, cx| i.set_value(text, w, cx));
        self.edit = Some(Edit::Key { name: key, kind });
        cx.notify();
    }
    fn file(&mut self, path: PathBuf, template: &str, w: &mut Window, cx: &mut Context<Self>) {
        match settings::read_document(&path) {
            Ok(original) => {
                self.editor.update(cx, |i, cx| {
                    i.set_value(
                        if original.is_empty() {
                            template.to_owned()
                        } else {
                            original.clone()
                        },
                        w,
                        cx,
                    )
                });
                self.edit = Some(Edit::File { path, original });
            }
            Err(e) => self.notice = format!("{e:#}"),
        }
        cx.notify();
    }
    fn save(&mut self, reset: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let text = if matches!(&self.edit,Some(Edit::Key{kind,..}) if ["string","enum","number","boolean"].contains(&kind.as_str()))
        {
            self.scalar.read(cx).value().to_string()
        } else {
            self.editor.read(cx).value().to_string()
        };
        let Some(edit) = &self.edit else {
            return;
        };
        let tx = self.tx.clone();
        match edit {
            Edit::Key { name, kind } => {
                let value = if reset {
                    None
                } else {
                    match settings::parse_value(kind, &text) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            self.notice = e.to_string();
                            cx.notify();
                            return;
                        }
                    }
                };
                let project = self.project;
                let path = settings::config_path(&self.base());
                let name = name.clone();
                let omp = self.omp.clone();
                let cwd = self.cwd.clone();
                std::thread::spawn(move || {
                    let result = if project {
                        settings::set_project(&path, &name, value)
                    } else {
                        match value {
                            Some(v) => settings::command(
                                &omp,
                                &cwd,
                                &["config", "set", &name, &v.to_string(), "--json"],
                            )
                            .map(|_| ()),
                            None => {
                                settings::command(&omp, &cwd, &["config", "reset", &name, "--json"])
                                    .map(|_| ())
                            }
                        }
                    };
                    let _ = tx.send(ResultEvent::Saved(result.map_err(|e| format!("{e:#}"))));
                });
            }
            Edit::File { path, original } => {
                let path = path.clone();
                let original = original.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(ResultEvent::Saved(
                        settings::save_document(&path, &original, &text)
                            .map_err(|e| format!("{e:#}")),
                    ));
                });
            }
        }
        self.busy = true;
        cx.notify();
    }
    fn plugin(&mut self, action: &'static str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        if action != "list" && (name.is_empty() || name.starts_with('-')) {
            self.notice = "请输入明确的插件名称或路径".into();
            cx.notify();
            return;
        }
        let omp = self.omp.clone();
        let cwd = self.cwd.clone();
        let project = self.project;
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let mut args = vec!["plugin", action];
            if action != "list" {
                args.push(&name);
            }
            if project {
                args.push("--local");
            }
            args.push("--json");
            let result = settings::command(&omp, &cwd, &args).and_then(|s| {
                if action == "list" {
                    Ok(s)
                } else {
                    let mut args = vec!["plugin", "list", "--json"];
                    if project {
                        args.push("--local");
                    }
                    settings::command(&omp, &cwd, &args)
                }
            });
            let _ = tx.send(ResultEvent::Plugins(result.map_err(|e| format!("{e:#}"))));
        });
        cx.notify();
    }
    fn create_prompt(&mut self, skill: bool, w: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_owned();
        if !skill && ["main", "sub"].contains(&name.to_lowercase().as_str()) {
            self.notice = "main 和 sub 是 OMP 保留名称，请选择其他子智能体名称".into();
            cx.notify();
            return;
        }
        if let Err(e) = settings::native_name(&name) {
            self.notice = e.to_string();
            cx.notify();
            return;
        }
        let path = if skill {
            self.base().join("skills").join(&name).join("SKILL.md")
        } else {
            self.base().join("agents").join(format!("{name}.md"))
        };
        let template = if skill {
            format!(
                "---\nname: {name}\ndescription: 描述这个技能适用的任务\n---\n\n在这里编写技能指令。\n"
            )
        } else {
            format!(
                "---\nname: {name}\ndescription: 描述这个子智能体的职责\nmodel: \"@default\"\n---\n\n在这里编写子智能体系统提示词。\n"
            )
        };
        self.file(path, &template, w, cx);
    }
    fn render_special(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_3();
        let ready = !self.catalog.root.as_os_str().is_empty() && !self.busy;
        match self.category {
            "常规" => {
                body = body
                    .child(div().text_sm().child("本机 OMP 可执行文件"))
                    .child(Input::new(&self.executable))
                    .child(
                        Button::new("save-executable")
                            .label("保存 OMP 路径")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.omp = this.executable.read(cx).value().trim().to_string();
                                cx.emit(SettingsEvent::OmpPath(this.omp.clone()));
                                this.reload();
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(
                        "新会话直接在项目目录执行，修改立即反映到该目录。不创建 Git 分支或工作树。",
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("当前执行目录：{}", self.cwd.display())),
                    );
            }
            "模型" => {
                body = body
                    .child(
                        div()
                            .text_sm()
                            .child("管理提供商连接、选择模型，并配置上下文长度与输出上限。"),
                    )
                    .child(
                        Button::new("models-config")
                            .primary()
                            .label("管理模型服务…")
                            .disabled(!ready || self.project)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.open_registry(false, w, cx)),
                            ),
                    );
            }
            "网络搜索" => {
                body = body
                    .child(
                        div()
                            .text_sm()
                            .child("选择搜索服务、注册凭据并调整备用顺序。"),
                    )
                    .child(
                        Button::new("web-config")
                            .primary()
                            .label("管理搜索服务…")
                            .disabled(!ready)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.open_registry(true, w, cx)),
                            ),
                    );
            }
            "LSP" | "MCP" => {
                let mcp = self.category == "MCP";
                body=body.child(div().text_sm().child(if mcp{"管理服务器地址、启动命令、环境变量及启用状态。OAuth 登录与诊断可使用 OMP 终端 /mcp。"}else{"管理语言服务器命令、文件类型、项目标记及 settings。OMP 保留内置服务器自动检测。"}))
                .child(Button::new("server-config").icon(IconName::Settings).label("管理服务器…").disabled(!ready).on_click(cx.listener(move|this,_,w,cx|{
                    let path=if mcp{this.base().join("mcp.json")}else{let base=this.base();["lsp.json",".lsp.json","lsp.yaml",".lsp.yaml","lsp.yml",".lsp.yml"].iter().map(|n|base.join(n)).find(|p|p.exists()).unwrap_or(base.join("lsp.json"))};
                    this.file(path,if mcp{"{\n  \"mcpServers\": {}\n}\n"}else{"{\n  \"servers\": {}\n}\n"},w,cx);
                })));
            }
            "角色与提示词" | "技能" => {
                let skill = self.category == "技能";
                body=body.child(div().text_sm().child(if skill{"原生技能保存在 skills/<名称>/SKILL.md。下面的 skills.ignoredSkills 可停用技能，skills.customDirectories 可注册额外目录。"}else{"modelRoles 负责 default、advisor 等角色与模型的关联；agents/*.md 定义带系统提示词的子智能体。两者按 OMP 原生机制分别保存。"}))
                .child(div().flex().gap_2().child(Input::new(&self.name)).child(Button::new("new-prompt").icon(IconName::Plus).label(if skill{"新建技能"}else{"新建子智能体"}).disabled(!ready).on_click(cx.listener(move|this,_,w,cx|this.create_prompt(skill,w,cx)))));
                if !skill {
                    body = body.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("role-mapping")
                                    .small()
                                    .label("角色与模型关联…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.key("modelRoles".into(), w, cx)
                                    })),
                            )
                            .child(
                                Button::new("system-prompt")
                                    .small()
                                    .label("主会话提示词…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.file(
                                            this.base().join("SYSTEM.md"),
                                            "请在这里编写主会话的自定义系统指令。",
                                            w,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("append-prompt")
                                    .small()
                                    .label("补充系统指令…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.file(
                                            this.base().join("APPEND_SYSTEM.md"),
                                            "请在这里编写追加到系统提示词的指令。",
                                            w,
                                            cx,
                                        )
                                    })),
                            ),
                    );
                }
                let key = format!(
                    "{}/{}",
                    if self.project { "project" } else { "global" },
                    if skill { "skills" } else { "agents" }
                );
                for path in self.catalog.prompt_files.get(&key).into_iter().flatten() {
                    let label = if skill {
                        path.parent().and_then(|p| p.file_name())
                    } else {
                        path.file_stem()
                    }
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                    let path = path.clone();
                    body = body.child(
                        Button::new(SharedString::from(format!("native-{label}")))
                            .ghost()
                            .accessibility_label(label.clone())
                            .child(div().flex_1().text_left().child(label))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.file(path.clone(), "", w, cx)
                            })),
                    );
                }
            }
            "插件" => {
                body=body.child(div().text_sm().child("通过本机 omp plugin 管理插件。安装会执行插件包安装流程；项目范围使用 --local。"))
                    .child(Input::new(&self.name));
                let mut actions = div().flex().flex_wrap().gap_2();
                for (label, action) in [
                    ("刷新", "list"),
                    ("安装", "install"),
                    ("启用", "enable"),
                    ("停用", "disable"),
                    ("卸载", "uninstall"),
                ] {
                    actions = actions.child(
                        Button::new(action)
                            .small()
                            .label(label)
                            .disabled(!ready)
                            .on_click(cx.listener(move |this, _, _, cx| this.plugin(action, cx))),
                    );
                }
                body = body.child(actions).child(
                    div()
                        .text_sm()
                        .font_family("Cascadia Code")
                        .child(self.plugins.clone()),
                );
            }
            _ => {}
        }
        body.into_any_element()
    }
}
impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(registry) = &self.registry {
            return div().size_full().child(registry.clone());
        }
        let theme = cx.theme().clone();
        let mut nav = div()
            .w_40()
            .h_full()
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .border_r_1()
            .border_color(theme.border);
        for category in CATEGORIES {
            nav = nav.child(
                SidebarMenu::new()
                    .child(
                        SidebarMenuItem::new(category)
                            .active(self.category == category)
                            .disable(self.edit.is_some())
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.category = category;
                                this.prepare_controls(w, cx);
                                cx.notify();
                            })),
                    )
                    .render(category, window, cx),
            );
        }
        let mut content = div().flex().flex_col().flex_1().min_w_0().h_full();
        content = content.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .p_4()
                .border_b_1()
                .border_color(theme.border)
                .child(div().text_lg().flex_1().child(self.category))
                .child(
                    Button::new("scope-global")
                        .small()
                        .label("全局")
                        .selected(!self.project)
                        .when(!self.project, |b| b.primary())
                        .disabled(self.edit.is_some())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.project = false;
                            this.prepare_controls(w, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("scope-project")
                        .small()
                        .label("项目")
                        .selected(self.project)
                        .when(self.project, |b| b.primary())
                        .disabled(self.edit.is_some())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.project = true;
                            this.prepare_controls(w, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("reload-settings")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .tooltip("重新读取 OMP 配置")
                        .disabled(self.edit.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reload();
                            cx.notify();
                        })),
                ),
        );
        let mut body = div()
            .id("settings-body")
            .flex_1()
            .min_h_0()
            .when(self.edit.is_some(), |d| d.overflow_y_scroll())
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if let Some(edit) = &self.edit {
            let (label, description) = match edit {
                Edit::Key { name, kind } => (
                    name.clone(),
                    format!("类型：{kind}。对象和数组使用 JSON；重置会移除当前范围的值。"),
                ),
                Edit::File { path, .. } => (
                    path.display().to_string(),
                    "编辑 OMP 原生配置。保存前校验格式并保留备份；注册项遵循本机 OMP 版本。".into(),
                ),
            };
            body = body
                .child(div().text_sm().font_family("Cascadia Code").child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description),
                );
            if matches!(edit,Edit::Key{kind,..} if ["string","enum","number"].contains(&kind.as_str()))
            {
                body = body.child(
                    div()
                        .max_w(rems(34.))
                        .child(Input::new(&self.scalar).aria_label("设置值")),
                );
            } else if !matches!(edit,Edit::Key{kind,..} if kind=="boolean") {
                body = body.child(Textarea::new(&self.editor).aria_label("设置值"));
            }
            if matches!(edit,Edit::Key{kind,..}if kind=="boolean") {
                let enabled = self.scalar.read(cx).value().trim() == "true";
                body = body.child(Switch::new("setting-toggle").checked(enabled).on_click(
                    cx.listener(|this, v: &bool, w, cx| {
                        this.scalar
                            .update(cx, |i, cx| i.set_value(v.to_string(), w, cx));
                        cx.notify();
                    }),
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("save-native")
                            .primary()
                            .label("保存")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save(false, cx))),
                    )
                    .when(matches!(edit, Edit::Key { .. }), |d| {
                        d.child(
                            Button::new("reset-native")
                                .label("重置此范围")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.save(true, cx))),
                        )
                    })
                    .child(
                        Button::new("cancel-native")
                            .ghost()
                            .label("取消编辑")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit = None;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            body = body.child(self.render_special(cx));
            if !["常规", "插件"].contains(&self.category) {
                body = body.child(Input::new(&self.search));
                let query = self.search.read(cx).value().to_lowercase();
                let keys = self
                    .catalog
                    .entries
                    .iter()
                    .filter(|(key, meta)| {
                        (settings::category(key) == self.category || !query.is_empty())
                            && (!self.project || !key.starts_with("auth.broker."))
                            && (query.is_empty()
                                || key.to_lowercase().contains(&query)
                                || meta["description"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_lowercase()
                                    .contains(&query))
                    })
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                let count = keys.len();
                body = body.child(
                    uniform_list(
                        "setting-rows",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|ix| this.setting_row(&keys[ix], cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .flex_1()
                    .min_h_0(),
                );
            }
        }
        content=content.child(body).child(div().p_3().border_t_1().border_color(theme.border).text_xs().text_color(theme.muted_foreground)
            .child(if self.busy{"正在保存…".to_string()}else if self.loading{"正在后台刷新配置…".to_string()}else{self.notice.clone()})
            .child(div().mt_2().child("OMP 持续更新，GUI 未覆盖的设置、认证及高级管理可在 OMP 终端中配置。项目设置以会话执行目录为准；环境变量与启动参数可能优先于文件。")));
        div()
            .flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(nav)
            .child(content)
    }
}

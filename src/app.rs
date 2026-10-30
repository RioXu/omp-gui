use crate::domain::Anchor;
use crate::{
    domain::*,
    engine::{Engine, Event},
    files,
    storage::{self, Persistence},
};
use gpui_kit::component::{
    ActiveTheme, Theme, ThemeMode, WindowExt,
    diff::{DiffFile, DiffLineRange, DiffSide, DiffState},
    input::{InputEvent, InputState, TextareaState},
    text::{RangeHighlight, TextViewState},
    tree::TreeState,
};
use gpui_kit::*;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

actions!(
    omp_gui,
    [SendPrompt, AddComment, TogglePanel, NewSession, OpenProject]
);

pub(crate) struct TextEntry {
    pub state: Entity<TextViewState>,
    pub body: String,
    pub source: Source,
}
pub(crate) enum FileEvent {
    List {
        session: String,
        changed: bool,
        result: Result<Vec<PathBuf>, String>,
    },
    Snapshot {
        session: String,
        result: Result<Snapshot, String>,
    },
    Current {
        session: String,
        path: PathBuf,
        result: Result<String, String>,
    },
}

pub(crate) struct Workspace {
    pub file_tree: Entity<TreeState>,
    pub change_tree: Entity<TreeState>,
    pub selection_dirty: bool,
    pub text_update_pending: bool,
    pub settings_view: Option<Entity<crate::settings_ui::SettingsView>>,
    pub creating_session: bool,
    pub show_archived: bool,
    pub diff: Option<Entity<DiffState>>,
    pub viewed_quote: Option<String>,
    pub db: Database,
    pub active: Option<usize>,
    pub persistence: Persistence,
    pub save_disabled: bool,
    pub input: Entity<TextareaState>,
    pub comment_input: Entity<TextareaState>,
    pub answer_input: Entity<InputState>,
    pub focus: FocusHandle,
    pub texts: HashMap<String, TextEntry>,
    pub panel_text: Option<TextEntry>,
    pub panel_open: bool,
    pub panel_tab: &'static str,
    pub files: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
    pub file_tx: mpsc::Sender<FileEvent>,
    pub file_rx: mpsc::Receiver<FileEvent>,
    pub viewed_snapshot: Option<String>,
    pub viewed_path: Option<PathBuf>,
    pub historical: bool,
    pub loading_files: bool,
    pub engine: Option<Engine>,
    pub connected: bool,
    pub running: bool,
    pub starting: bool,
    pub status: String,
    pub error: Option<String>,
    pub models: Vec<Value>,
    pub model_label: String,
    pub thinking: String,
    pub todos: Vec<Value>,
    pub agents: Vec<Value>,
    pub context_percent: f64,
    pub requests: VecDeque<Value>,
    pub activity: VecDeque<String>,
    pub selected: Option<Anchor>,
    pub selection_point: Point<Pixels>,
    pub comment_editor: Option<Anchor>,
    pub editing_comment: Option<String>,
    pub pending_open: bool,
    pub follow_up: Option<String>,
    pub in_flight_comments: Vec<String>,
    pub rpc_messages: HashMap<String, String>,
    pub pending_send: Option<(String, String)>,
    pub list: ListState,
    pub demo: bool,
    pub last_state: Instant,
    pub last_save: Instant,
    pub dirty: bool,
    pub _subscriptions: Vec<Subscription>,
    pub _data_lock: Option<std::fs::File>,
}
impl Workspace {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        demo: bool,
        project: Option<PathBuf>,
    ) -> Self {
        let lock = storage::lock_data();
        let loaded = storage::load();
        let save_disabled = loaded.is_err() || lock.is_err();
        let error = lock
            .as_ref()
            .err()
            .or_else(|| loaded.as_ref().err())
            .map(|e| format!("{e:#}"));
        let mut db = loaded.unwrap_or_default();
        if demo && db.sessions.is_empty() {
            db.sessions.push(demo_session());
            db.active = Some(db.sessions[0].id.clone());
        }
        let startup_project = project.and_then(|p| std::fs::canonicalize(p).ok());
        if let Some(p) = &startup_project
            && !db.projects.contains(p)
        {
            db.projects.push(p.clone());
        }
        let active = db
            .active
            .as_ref()
            .and_then(|id| db.sessions.iter().position(|s| &s.id == id));
        for session in &db.sessions {
            if !db.projects.contains(&session.project) {
                db.projects.push(session.project.clone());
            }
        }
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 7)
                .placeholder("继续描述任务，@ 引用文件，/ 使用命令")
        });
        if let Some(ix) = active {
            input.update(cx, |i, cx| {
                i.set_value(db.sessions[ix].draft.clone(), window, cx)
            });
        }
        let subscription = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(s) = this.session_mut()
            {
                s.draft = input.read(cx).value().to_string();
                this.dirty = true;
            }
            if matches!(
                event,
                InputEvent::PressEnter {
                    secondary: true,
                    ..
                }
            ) {
                cx.spawn_in(window, async move |this, cx| {
                    let _ = this.update_in(cx, |this, w, cx| this.send(w, cx));
                })
                .detach();
            }
        });
        let (file_tx, file_rx) = mpsc::channel();
        let mut this = Self {
            file_tree: cx.new(|cx| TreeState::new(cx)),
            change_tree: cx.new(|cx| TreeState::new(cx)),
            selection_dirty: false,
            text_update_pending: false,
            settings_view: None,
            creating_session: false,
            show_archived: false,
            diff: None,
            viewed_quote: None,
            db,
            active,
            persistence: Persistence::new(),
            save_disabled,
            input,
            comment_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(3, 8)
                    .placeholder("针对所选片段留下意见…")
            }),
            answer_input: cx.new(|cx| InputState::new(window, cx)),
            focus: cx.focus_handle(),
            texts: HashMap::new(),
            panel_text: None,
            panel_open: false,
            panel_tab: "变更",
            files: vec![],
            changed: vec![],
            file_tx,
            file_rx,
            viewed_snapshot: None,
            viewed_path: None,
            historical: false,
            loading_files: false,
            engine: None,
            connected: false,
            running: false,
            starting: false,
            status: if demo {
                "演示模式 · 不调用模型".into()
            } else {
                "选择项目以连接 OMP".into()
            },
            error,
            models: vec![],
            model_label: "默认模型".into(),
            thinking: "自动".into(),
            todos: vec![],
            agents: vec![],
            context_percent: 0.,
            requests: VecDeque::new(),
            activity: VecDeque::new(),
            selected: None,
            selection_point: Point::default(),
            comment_editor: None,
            editing_comment: None,
            pending_open: false,
            follow_up: None,
            in_flight_comments: vec![],
            rpc_messages: HashMap::new(),
            pending_send: None,
            list: ListState::new(0, ListAlignment::Bottom, px(600.)),
            demo,
            last_state: Instant::now(),
            last_save: Instant::now(),
            dirty: false,
            _subscriptions: vec![subscription],
            _data_lock: lock.ok(),
        };
        this.sync_texts(cx);
        let config_cwd = this
            .session()
            .map(|s| s.cwd().to_owned())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let config_omp = this.db.omp_path.clone();
        std::thread::spawn(move || {
            let _ = crate::settings::catalog(&config_omp, &config_cwd);
        });
        if let Some(project) = startup_project {
            this.new_project_session(project, window, cx);
        } else if !demo && this.active.is_some() {
            this.connect();
        }
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if view
                    .update_in(cx, |this, window, cx| this.poll(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        this
    }
    pub fn session(&self) -> Option<&Session> {
        self.active.and_then(|i| self.db.sessions.get(i))
    }
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.active.and_then(|i| self.db.sessions.get_mut(i))
    }
    pub fn persist(&mut self) {
        if self.save_disabled {
            return;
        }
        if let Err(e) = self.persistence.save(&self.db) {
            self.error = Some(e.to_string());
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }
    pub fn sync_texts(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        let messages = &self.db.sessions[ix].messages;
        let count = messages.len();
        for (ix, m) in messages.iter().enumerate() {
            if let Some(entry) = self.texts.get_mut(&m.id) {
                if entry.body != m.text {
                    if ix < self.list.item_count() {
                        self.list.splice(ix..ix + 1, 1);
                    }
                    entry.body = m.text.clone();
                    entry.state.update(cx, |s, cx| s.set_text(&m.text, cx));
                }
            } else {
                let state = cx.new(|cx| {
                    TextViewState::markdown(&m.text, cx).selectable(m.role == "assistant")
                });
                self.texts.insert(
                    m.id.clone(),
                    TextEntry {
                        state,
                        body: m.text.clone(),
                        source: Source::Message(m.id.clone()),
                    },
                );
            }
        }
        let old = self.list.item_count();
        if count > old {
            self.list.splice(old..old, count - old);
        } else if count < old {
            self.list.reset(count);
        }
    }
    pub fn connect(&mut self) {
        if self.demo || self.save_disabled {
            return;
        }
        self.engine = None;
        self.connected = false;
        self.running = false;
        self.starting = true;
        self.last_state = Instant::now();
        self.rpc_messages.clear();
        self.requests.clear();
        self.models.clear();
        let Some(s) = self.session() else {
            return;
        };
        if !s.cwd().is_dir() {
            self.starting = false;
            self.error = Some(
                "会话工作树已不在磁盘上；请恢复目录或新建会话。不会退回原项目目录执行。".into(),
            );
            return;
        }
        let result = Engine::start(
            &self.db.omp_path,
            s.cwd(),
            &storage::data_dir().join("omp-sessions").join(&s.id),
            s.omp_session
                .as_deref()
                .filter(|p| !s.messages.is_empty() || std::path::Path::new(p).is_file()),
        );
        match result {
            Ok(e) => {
                self.engine = Some(e);
                self.status = "正在连接本机 OMP…".into();
            }
            Err(e) => {
                self.starting = false;
                self.error = Some(format!("{e:#}"));
            }
        }
    }
    pub fn command(&mut self, frame: Value) {
        if let Some(e) = &self.engine
            && let Err(err) = e.send(frame)
        {
            self.error = Some(err.to_string());
        }
    }
    pub fn request_state(&mut self) {
        self.command(json!({"type":"get_state","id":"state"}));
        self.last_state = Instant::now();
    }
    pub fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.starting
            && self.requests.is_empty()
            && self.last_state.elapsed() > Duration::from_secs(60)
        {
            self.engine = None;
            self.starting = false;
            self.error = Some("OMP 连接超时，请检查路径或在终端启动 OMP 查看初始化问题".into());
            cx.notify();
        }
        let events = self
            .engine
            .as_ref()
            .map(|e| e.events.try_iter().take(200).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = !events.is_empty();
        for event in events {
            match event {
                Event::Frame(f) => self.on_frame(f, window, cx),
                Event::Error(e) => {
                    self.error = Some(e);
                }
                Event::Closed => {
                    self.connected = false;
                    self.starting = false;
                    self.running = false;
                    self.status = "OMP 已断开，可重新连接".into();
                }
            }
        }
        if self.text_update_pending {
            self.sync_texts(cx);
            self.text_update_pending = false;
        }
        let file_events = self.file_rx.try_iter().collect::<Vec<_>>();
        changed |= !file_events.is_empty();
        for event in file_events {
            self.on_file(event, cx);
        }

        for e in self.persistence.errors.try_iter() {
            self.error = Some(e);
            changed = true;
        }
        if self.connected && self.running && self.last_state.elapsed() > Duration::from_secs(3) {
            self.request_state();
            self.command(json!({"type":"get_subagents","id":"agents"}));
        }
        if self.selection_dirty && self.settings_view.is_none() && self.comment_editor.is_none() {
            self.selection_dirty = false;
            let mut selected = None;
            let active_ids = self
                .session()
                .map(|s| {
                    s.messages
                        .iter()
                        .filter(|m| m.role == "assistant")
                        .map(|m| m.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let entries = active_ids
                .iter()
                .filter_map(|id| self.texts.get(id))
                .chain(self.panel_text.as_ref().filter(|_| self.panel_open));
            for entry in entries {
                let state = entry.state.read(cx);
                if !state.is_selecting() {
                    let quote = state.selected_text();
                    if !quote.is_empty()
                        && let Some(range) = state.selected_source_range()
                    {
                        selected =
                            Anchor::new(entry.source.clone(), entry.body.clone(), range, quote)
                                .ok();
                    }
                }
            }
            if self.panel_open
                && let Some(diff) = &self.diff
            {
                let quote = diff.read(cx).selected_text(cx);
                if !quote.trim().is_empty()
                    && let Some(entry) = &self.panel_text
                {
                    selected = Anchor::from_quote(entry.source.clone(), entry.body.clone(), quote)
                        .ok()
                        .map(|mut a| {
                            a.code_lines = diff.read(cx).selected_lines().map(|r| CodeLines {
                                original: r.side() == DiffSide::Original,
                                end_original: r.end_side() == DiffSide::Original,
                                first: r.start(),
                                last: r.end(),
                            });
                            a
                        });
                }
            }
            if self
                .selected
                .as_ref()
                .map(|s| (&s.source, &s.range, &s.digest, &s.quote))
                != selected
                    .as_ref()
                    .map(|s| (&s.source, &s.range, &s.digest, &s.quote))
            {
                self.selected = selected;
                self.selection_point = window.mouse_position();
                changed = true;
            }
        }
        if self.dirty && self.last_save.elapsed() > Duration::from_millis(750) {
            self.persist();
        }
        if changed {
            cx.notify();
        }
    }
    pub fn on_frame(&mut self, f: Value, window: &mut Window, cx: &mut Context<Self>) {
        let kind = f["type"].as_str().unwrap_or("");
        match kind {
            "ready" => {
                self.command(
                    json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2}),
                );
                self.command(
                    json!({"type":"set_subagent_subscription","id":"subagents","level":"progress"}),
                );
                self.command(json!({"type":"get_available_models","id":"models"}));
                self.request_state();
            }
            "response" => {
                if f["success"] == false {
                    let command = f["command"].as_str().unwrap_or("");
                    if ![
                        "negotiate_protocol",
                        "set_subagent_subscription",
                        "get_subagents",
                    ]
                    .contains(&command)
                    {
                        self.error = Some(f["error"].as_str().unwrap_or("OMP 请求失败").into());
                    }
                    if command == "prompt" {
                        self.running = false;
                        self.pending_send = None;
                    }
                    return;
                }
                match f["command"].as_str().unwrap_or("") {
                    "get_state" => {
                        let d = &f["data"];
                        self.connected = true;
                        self.starting = false;
                        self.status = if self.running {
                            self.activity
                                .back()
                                .cloned()
                                .unwrap_or("OMP 正在思考…".into())
                        } else {
                            "本地 OMP 已连接".into()
                        };
                        self.model_label = d["model"]["id"].as_str().unwrap_or("未配置模型").into();
                        self.thinking = d["thinkingLevel"].as_str().unwrap_or("自动").into();
                        self.context_percent = d["contextUsage"]["percent"].as_f64().unwrap_or(0.);
                        self.todos = d["todoPhases"].as_array().cloned().unwrap_or_default();
                        if let Some(title) =
                            d["sessionName"].as_str().filter(|s| !s.trim().is_empty())
                            && let Some(s) = self.session_mut()
                            && s.title != title
                        {
                            s.title = title.into();
                            self.dirty = true;
                        }
                        if let Some(file) = d["sessionFile"].as_str()
                            && let Some(s) = self.session_mut()
                            && s.omp_session.as_deref() != Some(file)
                        {
                            s.omp_session = Some(file.into());
                            self.dirty = true;
                        }
                    }
                    "get_available_models" => {
                        self.models = f["data"]["models"]
                            .as_array()
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    "get_subagents" => {
                        self.agents = f["data"]["subagents"]
                            .as_array()
                            .or_else(|| f["data"]["agents"].as_array())
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    "prompt" => {
                        self.text_update_pending = true;
                        if let Some((id, text)) = self.pending_send.take() {
                            if f["id"] == id {
                                let follow = self.follow_up.take();
                                if let Some(s) = self.session_mut() {
                                    if s.title == "新建会话" {
                                        s.title = text.chars().take(28).collect();
                                        if s.title.trim().is_empty() {
                                            s.title = "评论跟进".into();
                                        }
                                    }
                                    let (_, ids) = s.submit(text, follow);
                                    self.in_flight_comments = ids;
                                }
                                self.input.update(cx, |i, cx| i.set_value("", window, cx));
                                if let Some(s) = self.session_mut() {
                                    s.draft.clear();
                                }
                                self.persist();
                            } else {
                                self.pending_send = Some((id, text));
                            }
                        }
                        if f["data"]["agentInvoked"] == false {
                            self.running = false;
                        }
                    }
                    "set_model" | "set_thinking_level" => self.request_state(),
                    _ => {}
                }
            }
            "agent_start" => {
                self.running = true;
                self.status = "OMP 正在处理…".into();
            }
            "session_info_update" => {
                if let Some(title) = f["title"].as_str().filter(|s| !s.trim().is_empty())
                    && let Some(s) = self.session_mut()
                {
                    s.title = title.into();
                    self.dirty = true;
                }
            }
            "agent_end" => {
                self.request_state();
                self.refresh_files(cx);
            }
            "session_settled" | "prompt_result" => {
                self.running = false;
                self.status = "本轮已完成".into();
                if f["status"] == "error" || f["status"] == "aborted" {
                    self.status = "本轮已停止".into();
                    if let Some(e) = f["error"].as_str() {
                        self.error = Some(e.into());
                    }
                }
                self.request_state();
                self.refresh_files(cx);
                self.persist();
            }
            "message_start" | "message_update" | "message_end" => {
                self.text_update_pending = true;
                let m = &f["message"];
                if m["role"] != "assistant" {
                    return;
                }
                let text = crate::engine::message_text(m);
                if text.is_empty() {
                    return;
                }
                let rid = f["messageId"].as_str().unwrap_or("stream").to_string();
                let mid = self.rpc_messages.entry(rid).or_insert_with(id).clone();
                let linked = self.in_flight_comments.clone();
                if let Some(s) = self.session_mut() {
                    if let Some(existing) = s.messages.iter_mut().find(|m| m.id == mid) {
                        existing.text = text;
                    } else {
                        let mut msg = Message::new("assistant", text);
                        msg.id = mid.clone();
                        s.messages.push(msg);
                        for c in &mut s.comments {
                            if linked.contains(&c.id) && !c.reply_ids.contains(&mid) {
                                c.reply_ids.push(mid.clone());
                            }
                        }
                    }
                }
                self.dirty = true;
            }
            "tool_execution_start" => {
                let name = f["toolName"].as_str().unwrap_or("工具");
                self.activity.push_back(format!("正在执行 {name}"));
                if self.activity.len() > 200 {
                    self.activity.pop_front();
                }
            }
            "tool_execution_end" => {
                self.request_state();
            }
            "subagent_lifecycle" | "subagent_progress" => {
                self.command(json!({"type":"get_subagents","id":"agents"}));
            }
            "extension_ui_request" => {
                match f["method"].as_str().unwrap_or("") {
                    "select" | "confirm" | "input" | "editor" => {
                        if f["secret"] == true {
                            self.command(json!({"type":"extension_ui_response","id":f["id"],"cancelled":true}));
                            self.error = Some("请在 OMP 终端完成敏感凭据输入后重新连接".into());
                        } else {
                            self.requests.push_back(f);
                        }
                    }
                    "cancel" => {
                        self.requests.retain(|r| r["id"] != f["targetId"]);
                    }
                    "notify" => {
                        self.status = f["message"].as_str().unwrap_or("").into();
                    }
                    "set_editor_text" => {
                        if let Some(text) = f["text"].as_str() {
                            self.input
                                .update(cx, |i, cx| i.set_value(text.to_string(), window, cx));
                        }
                    }
                    _ => {}
                }
            }
            "notice" | "extension_error" | "rpc_frame_error" => {
                self.error = Some(
                    f["message"]
                        .as_str()
                        .or_else(|| f["error"].as_str())
                        .unwrap_or("OMP 返回一条异常通知")
                        .into(),
                );
            }
            _ => {}
        }
    }
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            self.error = Some("当前窗口为只读状态，无法发送或保存。请先解决数据目录问题。".into());
            cx.notify();
            return;
        }
        if self.running || self.pending_send.is_some() {
            return;
        }
        let text = self.input.read(cx).value().trim_end().to_string();
        let Some(s) = self.session() else {
            self.open_project(window, cx);
            return;
        };
        if text.trim().is_empty() && !s.comments.iter().any(|c| !c.sent) {
            return;
        }
        let prompt = s.prompt(&text, self.follow_up.as_deref());
        if self.demo {
            let follow = self.follow_up.take();
            if let Some(s) = self.session_mut() {
                s.submit(text, follow);
            }
            self.input.update(cx, |i, cx| i.set_value("", window, cx));
            self.status = "演示：已保留评论记录，未调用模型".into();
            self.sync_texts(cx);
            self.persist();
            cx.notify();
            return;
        }
        if !self.connected {
            self.error = Some("OMP 尚未连接，请稍后发送或在设置中重新连接".into());
            cx.notify();
            return;
        }
        let request_id = id();
        if let Some(e) = &self.engine {
            match e.send(json!({"type":"prompt","id":request_id,"message":prompt})) {
                Ok(()) => {
                    self.pending_send = Some((request_id, text));
                    self.running = true;
                    self.status = "正在发送…".into();
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        cx.notify();
    }
    pub fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.creating_session || self.save_disabled {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 OMP 工作目录".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = this.update_in(cx, |this, window, cx| {
                    if !this.db.projects.contains(&path) {
                        this.db.projects.push(path);
                    }
                    this.persist();
                    let _ = window;
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.running
            || self.creating_session
            || self.db.sessions.get(ix).is_none_or(|s| s.deleted)
        {
            return;
        }
        self.persist();
        self.active = Some(ix);
        self.db.active = Some(self.db.sessions[ix].id.clone());
        self.texts.clear();
        self.list.reset(0);
        self.panel_text = None;
        self.diff = None;
        self.viewed_quote = None;
        self.settings_view = None;
        self.viewed_path = None;
        self.viewed_snapshot = None;
        self.selected = None;
        self.follow_up = None;
        self.files.clear();
        self.changed.clear();
        self.todos.clear();
        self.agents.clear();
        self.error = None;
        self.input.update(cx, |i, cx| {
            i.set_value(self.db.sessions[ix].draft.clone(), window, cx)
        });
        self.sync_texts(cx);
        self.connect();
        self.refresh_files(cx);
        self.persist();
        cx.notify();
    }
    pub fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running || self.creating_session || self.save_disabled {
            return;
        }
        if let Some(s) = self.session() {
            self.new_project_session(s.project.clone(), window, cx);
        } else {
            self.open_project(window, cx);
        }
    }
    pub fn new_project_session(
        &mut self,
        project: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running || self.creating_session || self.save_disabled {
            return;
        }
        if !project.is_dir() {
            self.error = Some("项目目录不存在".into());
            cx.notify();
            return;
        }
        self.db.sessions.push(Session::new(project));
        self.activate(self.db.sessions.len() - 1, window, cx);
    }
    pub fn manage_session(
        &mut self,
        sid: &str,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running || self.save_disabled {
            return;
        }
        let Some(ix) = self.db.sessions.iter().position(|s| s.id == sid) else {
            return;
        };
        let active_id = self.session().map(|s| s.id.clone());
        let clear = self.active == Some(ix) && (delete || !self.db.sessions[ix].archived);
        if delete {
            self.db.remove_session(sid);
        } else {
            self.db.sessions[ix].archived = !self.db.sessions[ix].archived;
        }
        if clear {
            self.engine = None;
            self.connected = false;
            self.starting = false;
            self.active = None;
            self.db.active = None;
            self.texts.clear();
            self.list.reset(0);
            self.panel_text = None;
            self.diff = None;
            self.input.update(cx, |i, cx| i.set_value("", window, cx));
        } else {
            self.active = active_id.and_then(|id| self.db.sessions.iter().position(|s| s.id == id));
        }
        self.persist();
        cx.notify();
    }
    pub fn request_delete_session(&mut self, sid: String, w: &mut Window, cx: &mut Context<Self>) {
        use gpui_kit::component::{
            button::{Button, ButtonVariants},
            dialog::{DialogAction, DialogClose, DialogFooter},
        };
        let weak = cx.entity().downgrade();
        w.open_dialog(cx,move|d,_,_|{
            let weak=weak.clone();let sid=sid.clone();
            d.title("删除会话？").child("将移除 GUI 中的消息、评论和历史快照。项目文件及 OMP 独立保存的原生会话文件不会删除。")
                .footer(DialogFooter::new().child(DialogClose::new().trigger(|b|b.label("取消")))
                    .child(DialogAction::new().child(Button::new("confirm-delete-session").danger().label("删除会话"))))
                .on_ok(move|_,w,cx|{let _=weak.update(cx,|this,cx|this.manage_session(&sid,true,w,cx));true})
        });
    }
    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        if self.loading_files {
            return;
        }
        if self.demo {
            if let Some(s) = self.session() {
                self.changed = s.snapshots.values().map(|s| s.path.clone()).collect();
                self.changed.sort();
                self.changed.dedup();
                self.files = self.changed.clone();
                self.file_tree.update(cx, |t, cx| {
                    t.set_items(crate::file_tree::items(&self.files), cx)
                });
                self.change_tree.update(cx, |t, cx| {
                    t.set_items(crate::file_tree::items(&self.changed), cx)
                });
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let project = s.cwd().to_owned();
        let sid = s.id.clone();
        let tx = self.file_tx.clone();
        self.loading_files = true;
        std::thread::spawn(move || {
            for changed in [true, false] {
                let result = files::list(&project, changed).map_err(|e| format!("{e:#}"));
                let _ = tx.send(FileEvent::List {
                    session: sid.clone(),
                    changed,
                    result,
                });
            }
        });
    }
    pub fn open_file(&mut self, path: PathBuf) {
        if self.demo {
            let snapshot = self
                .session()
                .and_then(|s| s.snapshots.values().find(|n| n.path == path))
                .cloned();
            if let Some(snapshot) = snapshot {
                let _ = self.file_tx.send(FileEvent::Snapshot {
                    session: self.session().unwrap().id.clone(),
                    result: Ok(snapshot),
                });
            }
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let sid = s.id.clone();
        let root = s.cwd().to_owned();
        let tx = self.file_tx.clone();
        self.status = "正在读取文件…".into();
        std::thread::spawn(move || {
            let result = files::snapshot(&root, &path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(FileEvent::Snapshot {
                session: sid,
                result,
            });
        });
    }
    pub fn on_file(&mut self, event: FileEvent, cx: &mut Context<Self>) {
        let sid = self.session().map(|s| s.id.clone()).unwrap_or_default();
        match event {
            FileEvent::List {
                session,
                changed,
                result,
            } if session == sid => {
                self.loading_files = false;
                match result {
                    Ok(paths) => {
                        let tree = if changed {
                            self.change_tree.clone()
                        } else {
                            self.file_tree.clone()
                        };
                        let current = if changed { &self.changed } else { &self.files };
                        if current != &paths {
                            tree.update(cx, |t, cx| {
                                t.set_items(crate::file_tree::items(&paths), cx)
                            });
                        }
                        if changed {
                            self.changed = paths
                        } else {
                            self.files = paths
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            FileEvent::Snapshot { session, result } if session == sid => match result {
                Ok(snapshot) => {
                    self.viewed_quote = None;
                    self.diff = if self.panel_tab == "变更" && !snapshot.patch.is_empty() {
                        DiffFile::parse(&snapshot.patch)
                            .ok()
                            .map(|files| cx.new(|cx| DiffState::new(files, cx)))
                    } else {
                        None
                    };
                    self.selected = None;
                    self.viewed_path = Some(snapshot.path.clone());
                    self.viewed_snapshot = Some(snapshot.id.clone());
                    self.historical = false;
                    let body = if self.panel_tab == "文件" {
                        fenced(&snapshot.text, "")
                    } else {
                        snapshot.document()
                    };
                    let source = Source::File {
                        path: snapshot.path.clone(),
                        snapshot: snapshot.id.clone(),
                    };
                    self.panel_text = Some(TextEntry {
                        state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true)),
                        body,
                        source,
                    });
                    if let Some(entry) = &self.panel_text {
                        highlight_diff(entry, cx);
                    }
                    if let Some(s) = self.session_mut() {
                        s.snapshots.insert(snapshot.id.clone(), snapshot);
                    }
                    self.dirty = true;
                    self.status = "文件已载入".into();
                }
                Err(e) => self.error = Some(e),
            },
            FileEvent::Current {
                session,
                path,
                result,
            } if session == sid => match result {
                Ok(text) => {
                    self.viewed_quote = None;
                    self.diff = None;
                    let body = fenced(&text, "");
                    self.viewed_path = Some(path.clone());
                    self.historical = false;
                    self.panel_text = Some(TextEntry {
                        state: cx.new(|cx| TextViewState::markdown(&body, cx).selectable(false)),
                        body,
                        source: Source::File {
                            path,
                            snapshot: String::new(),
                        },
                    });
                }
                Err(e) => self.error = Some(e),
            },
            _ => {}
        }
    }
    pub fn begin_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(a) = self.selected.take() {
            self.comment_editor = Some(a);
            self.editing_comment = None;
            self.comment_input
                .update(cx, |i, cx| i.set_value("", window, cx));
            self.comment_input.focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }
    pub fn save_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.comment_input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(anchor) = self.comment_editor.take() else {
            return;
        };
        let editing = self.editing_comment.take();
        if let Some(s) = self.session_mut() {
            if let Some(id) = editing {
                if let Some(c) = s.comments.iter_mut().find(|c| c.id == id && !c.sent) {
                    c.text = text;
                }
            } else {
                s.comments.push(Comment::new(anchor, text));
            }
        }
        self.pending_open = true;
        self.selected = None;
        gpui_kit::base::TextSelection::clear(window, cx);
        self.persist();
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub fn open_anchor(&mut self, anchor: Anchor, cx: &mut Context<Self>) {
        self.diff = None;
        if let Source::Message(id) = &anchor.source
            && let Some((ix, state)) = self
                .session()
                .and_then(|s| s.messages.iter().position(|m| &m.id == id))
                .and_then(|ix| {
                    self.texts
                        .get(id)
                        .filter(|e| e.body == anchor.document)
                        .map(|e| (ix, e.state.clone()))
                })
        {
            let color = cx.theme().selection;
            state.update(cx, |s, cx| {
                if let Some(range) = s.rendered_text().range_for_source(anchor.range.clone()) {
                    let _ = s.set_range_highlights([RangeHighlight::new(range.clone(), color)], cx);
                    let _ = s.reveal_range(range, cx);
                }
            });
            self.list.scroll_to_reveal_item(ix);
            self.selected = None;
            cx.notify();
            return;
        }
        self.panel_open = true;
        self.panel_tab = "变更";
        self.historical = true;
        self.viewed_path = match &anchor.source {
            Source::File { path, snapshot } => {
                self.viewed_snapshot = Some(snapshot.clone());
                Some(path.clone())
            }
            _ => {
                self.viewed_snapshot = None;
                None
            }
        };
        self.viewed_quote = Some(anchor.quote.clone());
        if let Source::File { path, snapshot } = &anchor.source
            && let Some(stored) = self.session().and_then(|s| s.snapshots.get(snapshot))
            && let Ok(files) = DiffFile::parse(&stored.patch)
            && !files.is_empty()
        {
            let state = cx.new(|cx| DiffState::new(files, cx));
            if let Some(lines) = &anchor.code_lines {
                let range = DiffLineRange::new(
                    path.to_string_lossy().replace('\\', "/"),
                    if lines.original {
                        DiffSide::Original
                    } else {
                        DiffSide::Modified
                    },
                    lines.first,
                    lines.last,
                )
                .with_end_side(if lines.end_original {
                    DiffSide::Original
                } else {
                    DiffSide::Modified
                });
                state.update(cx, |d, cx| {
                    d.set_selected_lines(Some(range), cx);
                });
            }
            self.diff = Some(state);
        }
        let body = anchor.document.clone();
        let state = cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true));
        let color = cx.theme().selection;
        state.update(cx, |s, cx| {
            if anchor.exact_range
                && let Some(r) = s.rendered_text().range_for_source(anchor.range.clone())
            {
                let _ = s.set_range_highlights([RangeHighlight::new(r.clone(), color)], cx);
                let _ = s.reveal_range(r, cx);
            }
        });
        self.panel_text = Some(TextEntry {
            state,
            body,
            source: anchor.source,
        });
        self.selected = None;
        cx.notify();
    }
    pub fn open_current(&mut self) {
        let Some(path) = self.viewed_path.clone() else {
            return;
        };
        if self.demo {
            self.open_file(path);
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let sid = s.id.clone();
        let root = s.cwd().to_owned();
        let tx = self.file_tx.clone();
        std::thread::spawn(move || {
            let result = files::read(&root, &path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(FileEvent::Current {
                session: sid,
                path,
                result,
            });
        });
    }
    pub fn answer_request(&mut self, value: Option<String>, confirmed: Option<bool>) {
        if let Some(r) = self.requests.pop_front() {
            let mut f = json!({"type":"extension_ui_response","id":r["id"]});
            if let Some(v) = value {
                f["value"] = json!(v);
            } else if let Some(v) = confirmed {
                f["confirmed"] = json!(v);
            } else {
                f["cancelled"] = json!(true);
            }
            self.command(f);
        }
    }
    pub fn settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            return;
        }
        let cwd = self
            .session()
            .map(|s| s.cwd().to_owned())
            .or_else(|| self.db.projects.first().cloned())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let view = cx.new(|cx| {
            crate::settings_ui::SettingsView::new(self.db.omp_path.clone(), cwd, window, cx)
        });
        self._subscriptions
            .push(cx.subscribe(&view, |this, _, event, cx| {
                match event {
                    crate::settings_ui::SettingsEvent::OmpPath(path) => {
                        this.db.omp_path = path.clone();
                        this.persist();
                        if !this.running {
                            this.connect();
                        }
                    }
                    crate::settings_ui::SettingsEvent::Saved => {
                        this.request_state();
                        this.command(json!({"type":"get_available_models","id":"models"}));
                    }
                    crate::settings_ui::SettingsEvent::Reconnect => {
                        if !this.running {
                            this.connect();
                        }
                    }
                }
                cx.notify();
            }));
        self.settings_view = Some(view);
        cx.notify();
    }
    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dark = !cx.theme().is_dark();
        Theme::change(
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        self.db.theme = if dark { "dark" } else { "light" }.into();
        apply_accent(cx);
        self.persist();
    }
}
pub(crate) fn apply_accent(cx: &mut App) {
    Theme::update(cx, |t| {
        let blue = gpui_kit::rgb(if t.is_dark() { 0x608fff } else { 0x2563eb }).into();
        t.primary = blue;
        t.primary_hover = blue;
        t.primary_active = blue;
        t.primary_foreground = gpui_kit::rgb(0xffffff).into();
        t.button_primary = blue;
        t.button_primary_hover = blue;
        t.button_primary_active = blue;
        t.button_primary_foreground = t.primary_foreground;
        t.sidebar_accent = gpui_kit::rgb(if t.is_dark() { 0x213454 } else { 0xdbeafe }).into();
        t.sidebar_accent_foreground =
            gpui_kit::rgb(if t.is_dark() { 0xbcd3ff } else { 0x1d4ed8 }).into();
        t.accent = t.sidebar_accent;
        t.accent_foreground = t.sidebar_accent_foreground;
        t.list_active = t.sidebar_accent;
        t.list_active_border = t.primary;
        t.secondary_active = t.sidebar_accent;
        t.button_active = t.sidebar_accent;
        t.tab_active = t.sidebar_accent;
        t.tab_active_foreground = t.sidebar_accent_foreground;
    });
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.persist();
    }
}

fn highlight_diff(entry: &TextEntry, cx: &mut App) {
    if !entry
        .body
        .lines()
        .next()
        .is_some_and(|l| l.ends_with("diff"))
    {
        return;
    }
    let added = cx.theme().success.opacity(0.14);
    let removed = cx.theme().danger.opacity(0.12);
    entry.state.update(cx, |s, cx| {
        let rendered = s.rendered_text();
        let mut offset = 0;
        let mut highlights = Vec::new();
        for line in entry.body.split_inclusive('\n') {
            let color = if line.starts_with('+') && !line.starts_with("+++") {
                Some(added)
            } else if line.starts_with('-') && !line.starts_with("---") {
                Some(removed)
            } else {
                None
            };
            if let Some(color) = color
                && let Some(range) =
                    rendered.range_for_source(offset..offset + line.trim_end_matches('\n').len())
            {
                highlights.push(RangeHighlight::new(range, color));
            }
            offset += line.len();
        }
        let _ = s.set_range_highlights(highlights, cx);
    });
}

fn demo_session() -> Session {
    let mut s = Session::new(std::env::current_dir().unwrap_or_default());
    s.title = "优化会话列表 · 演示".into();
    s.messages.push(Message::new(
        "user",
        "会话多的时候切换项目有点慢，优化一下，同时保留最近打开的会话。".into(),
    ));
    let m=Message::new("assistant","会话列表已改为按项目分组，并保留最近访问顺序和当前选中状态。\n\n切换项目时会复用已加载的记录，重复打开同一项目不再触发完整扫描。性能提升还需要专项测量。".into());
    let phrase = "重复打开同一项目不再触发完整扫描";
    let start = m.text.find(phrase).unwrap();
    let a = Anchor::new(
        Source::Message(m.id.clone()),
        m.text.clone(),
        start..start + phrase.len(),
        phrase.into(),
    )
    .unwrap();
    s.messages.push(m);
    let snap = Snapshot::new(
        "src/ui/sidebar.rs".into(),
        "fn sessions() {\n    store.scan_all()\n}\n".into(),
        "fn sessions() {\n    store.cached_sessions()\n}\n".into(),
    );
    let doc = snap.document();
    let start = doc.find("store.cached_sessions()").unwrap();
    let code = Anchor::new(
        Source::File {
            path: snap.path.clone(),
            snapshot: snap.id.clone(),
        },
        doc,
        start..start + "store.cached_sessions()".len(),
        "store.cached_sessions()".into(),
    )
    .unwrap();
    s.snapshots.insert(snap.id.clone(), snap);
    let c1 = Comment::new(a, "这个结论有测试数据支撑吗？".into());
    let c2 = Comment::new(code, "缓存失效时会刷新吗？".into());
    s.comments.extend([c1, c2]);
    s.submit(String::new(), None);
    let reply = Message::new(
        "assistant",
        "还没有专项性能数据，需要补充测量。缓存失效检查也需要继续验证，评论保留为待确认。".into(),
    );
    for c in &mut s.comments {
        c.reply_ids.push(reply.id.clone());
    }
    s.messages.push(reply);
    s
}

use super::*;
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
        db.normalize_projects();
        db.discard_blank_sessions(None);
        for session in &mut db.sessions {
            session.recover_unconfirmed_queue();
        }
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
                cx.notify();
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
            transcript: vec![],
            tool_diffs: HashMap::new(),
            stopping: false,
            interrupted_run: false,
            run_message_start: 0,
            submitted_inputs: vec![],
            queued_inputs: vec![],
            steered_inputs: vec![],
            steering_request: None,
            expanded_process: Default::default(),
            file_tree: cx.new(|cx| TreeState::new(cx)),
            change_tree: cx.new(|cx| TreeState::new(cx)),
            selection_dirty: false,
            text_update_pending: false,
            settings_view: None,
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
            preview_request: None,
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
            context_limit: 0,
            active_tools: HashMap::new(),
            requests: VecDeque::new(),
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
}

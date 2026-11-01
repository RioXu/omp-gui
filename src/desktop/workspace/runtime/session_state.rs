//! Per-session OMP transport, composer and transcript state.
use super::*;

pub(in crate::desktop::workspace) struct SessionRuntime {
    pub model_controls: ModelControlState,
    pub answer_input: Entity<InputState>,
    pub composer_state: ComposerState,
    pub collaboration: runtime::collaboration::CollaborationState,
    pub transcript: Vec<TranscriptRow>,
    pub tool_diffs: HashMap<String, Entity<DiffState>>,
    pub stopping: bool,
    pub interrupted_run: bool,
    pub run_message_start: usize,
    pub submitted_inputs: Vec<SubmittedInput>,
    pub expanded_process: std::collections::HashSet<String>,
    pub text_update_pending: bool,
    pub texts: HashMap<String, TextEntry>,
    pub engine: Option<Box<dyn crate::infrastructure::omp::SessionConnection>>,
    pub connection_result: Option<
        mpsc::Receiver<Result<Box<dyn crate::infrastructure::omp::SessionConnection>, String>>,
    >,
    pub connected: bool,
    pub reconnecting: bool,
    pub running: bool,
    pub starting: bool,
    pub status: String,
    pub error: Option<String>,
    pub todos: Vec<Value>,
    pub agents: Vec<Value>,
    pub context_percent: f64,
    pub context_limit: u64,
    pub active_tools: HashMap<String, String>,
    pub requests: VecDeque<Value>,
    pub follow_up: Option<String>,
    pub in_flight_comments: Vec<String>,
    pub rpc_messages: HashMap<String, String>,
    pub pending_send: Option<PendingSubmission>,
    pub preparing: Option<crate::desktop::workspace::conversation::submission::PreparingSubmission>,
    pub queued_inputs: Vec<String>,
    pub steered_inputs: Vec<String>,
    pub steering_request: Option<String>,
    pub list: ListState,
    pub last_state: Instant,
    pub last_activity: Instant,
    pub context_used: Option<u64>,
    pub _input_subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub(in crate::desktop::workspace) fn new_session_runtime(
        window: &mut Window,
        cx: &mut Context<Self>,
        demo: bool,
        draft: String,
        error: Option<String>,
    ) -> SessionRuntime {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 7)
                .placeholder("继续描述任务，@ 引用文件，/ 使用命令")
        });
        input.update(cx, |i, cx| i.set_value(draft, window, cx));
        let subscription = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            if this.runtime.composer_state.input.entity_id() != input.entity_id() {
                return;
            }
            if matches!(event, InputEvent::Change) {
                this.runtime.composer_state.selected_completion = 0;
                this.runtime.composer_state.dismissed = false;
                if input.read(cx).value().contains('@') && this.document_state.files.is_empty() {
                    this.refresh_files(cx);
                }
                cx.notify();
            }
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
                let owner = input.entity_id();
                cx.spawn_in(window, async move |this, cx| {
                    let _ = this.update_in(cx, |this, w, cx| {
                        if this.runtime.composer_state.input.entity_id() == owner {
                            this.send(w, cx);
                        }
                    });
                })
                .detach();
            }
        });
        let input_observation = cx.observe(&input, |_, _, cx| cx.notify());
        SessionRuntime {
            model_controls: ModelControlState {
                available: vec![],
                current: Value::Null,
                levels: vec![],
                levels_request: None,
                auto: false,
                entries_cursor: None,
                pending: None,
                label: "默认模型".into(),
                thinking: "自动".into(),
            },
            answer_input: cx.new(|cx| InputState::new(window, cx)),
            composer_state: ComposerState {
                input,
                commands: vec![],
                selected_completion: 0,
                dismissed: false,
            },
            collaboration: Default::default(),
            transcript: vec![],
            tool_diffs: HashMap::new(),
            stopping: false,
            interrupted_run: false,
            run_message_start: 0,
            submitted_inputs: vec![],
            expanded_process: Default::default(),
            text_update_pending: false,
            texts: HashMap::new(),
            engine: None,
            connection_result: None,
            connected: false,
            reconnecting: false,
            running: false,
            starting: false,
            status: if demo {
                "演示模式 · 不调用模型".into()
            } else {
                "选择项目以连接 OMP".into()
            },
            error,
            todos: vec![],
            agents: vec![],
            context_percent: 0.,
            context_limit: 0,
            active_tools: HashMap::new(),
            requests: VecDeque::new(),
            follow_up: None,
            in_flight_comments: vec![],
            rpc_messages: HashMap::new(),
            pending_send: None,
            preparing: None,
            queued_inputs: vec![],
            steered_inputs: vec![],
            steering_request: None,
            list: ListState::new(0, ListAlignment::Bottom, px(600.)),
            last_state: Instant::now(),
            last_activity: Instant::now(),
            context_used: None,
            _input_subscriptions: vec![subscription, input_observation],
        }
    }
}

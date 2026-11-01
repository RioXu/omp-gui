use super::*;

impl Workspace {
    pub fn connect(&mut self) {
        if self.demo || self.save_disabled {
            return;
        }
        if self.session_sync.store.is_none() {
            self.scan_sessions();
            return;
        }
        self.runtime.engine = None;
        self.runtime.connected = false;
        self.runtime.reconnecting = false;
        self.runtime.running = false;
        self.runtime.active_tools.clear();
        self.runtime.context_limit = 0;
        self.runtime.context_used = None;
        self.runtime.stopping = false;
        self.runtime.interrupted_run = false;
        self.runtime.submitted_inputs.clear();
        self.runtime.queued_inputs.clear();
        self.runtime.steered_inputs.clear();
        self.runtime.steering_request = None;
        self.runtime.pending_send = None;
        self.runtime.starting = true;
        self.runtime.last_state = Instant::now();
        self.runtime.rpc_messages.clear();
        self.runtime.requests.clear();
        self.runtime.model_controls.available.clear();
        self.runtime.model_controls.current = Value::Null;
        self.runtime.model_controls.levels.clear();
        self.runtime.model_controls.levels_request = None;
        self.runtime.model_controls.auto = false;
        self.runtime.model_controls.entries_cursor = None;
        self.runtime.model_controls.pending = None;
        self.runtime.composer_state.commands.clear();
        let Some(s) = self.session() else {
            return;
        };
        if matches!(s.host_cwd(), omp_core::environment::HostPath::Local(path) if !path.is_dir()) {
            self.runtime.starting = false;
            self.runtime.error = Some(
                "会话工作树已不在磁盘上；请恢复目录或新建会话。不会退回原项目目录执行。".into(),
            );
            return;
        }
        let Some(store) = self.session_sync.store.as_ref() else {
            self.runtime.starting = false;
            self.scan_sessions();
            return;
        };
        let store = store.clone();
        let root = s.host_cwd();
        let native_root = s.native_root.clone();
        let path = s.omp_session.clone();
        let native_id = s.native_id.clone();
        let (tx, rx) = mpsc::channel();
        self.runtime.connection_result = Some(rx);
        self.runtime.error = None;
        self.runtime.status = "正在连接 OMP…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                anyhow::ensure!(
                    native_root.as_deref() == Some(store.root.text().as_str()),
                    "会话尚未完成原生存储迁移"
                );
                let path = path
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("会话尚未绑定原生记录"))?;
                store.connect(&root, path, native_id.as_deref().unwrap_or_default())
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(result);
        });
    }
    pub fn command(&mut self, frame: Value) {
        if let Some(e) = &self.runtime.engine
            && let Err(err) = e.send(frame)
        {
            self.runtime.error = Some(err.to_string());
        }
    }
    pub fn request_state(&mut self) {
        self.command(json!({"type":"get_state","id":"state"}));
        self.runtime.last_state = Instant::now();
    }
}

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
        if !s.cwd().is_dir() {
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
        let result = (|| -> anyhow::Result<_> {
            anyhow::ensure!(
                s.native_root.as_deref() == Some(store.root.to_string_lossy().as_ref()),
                "会话尚未完成原生存储迁移"
            );
            let path = s
                .omp_session
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("会话尚未绑定原生记录"))?;
            anyhow::ensure!(
                std::path::Path::new(path).is_file(),
                "原生会话已移除，正在同步列表"
            );
            store.available(s.native_id.as_deref().unwrap_or_default())?;
            OmpClient::start_project(&self.db.omp_path, s.cwd(), path)
        })();
        match result {
            Ok(e) => {
                self.runtime.engine = Some(Box::new(e));
                self.runtime.error = None;
                self.runtime.status = "正在连接本机 OMP…".into();
            }
            Err(e) => {
                self.runtime.starting = false;
                self.runtime.error = Some(format!("{e:#}"));
                self.runtime.status = "原生会话暂不可用 · 可查看历史".into();
            }
        }
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

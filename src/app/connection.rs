use super::*;

impl Workspace {
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
}

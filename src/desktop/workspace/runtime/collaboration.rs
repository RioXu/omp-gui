use super::*;
use crate::domain::activity::{merge_agent, merge_tool_activity};
use crate::infrastructure::omp::sessions::SessionStore;

struct TranscriptResult {
    session: String,
    agent: String,
    request: String,
    result: Result<Vec<Message>, String>,
}
pub(in crate::desktop::workspace) struct CollaborationState {
    pub selected: Option<String>,
    pub messages: Vec<Message>,
    pub error: Option<String>,
    pub loading: bool,
    request: String,
    last_read: Instant,
    tx: mpsc::Sender<TranscriptResult>,
    rx: mpsc::Receiver<TranscriptResult>,
}
impl Default for CollaborationState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            selected: None,
            messages: vec![],
            error: None,
            loading: false,
            request: String::new(),
            last_read: Instant::now(),
            tx,
            rx,
        }
    }
}
impl Workspace {
    pub(in crate::desktop::workspace) fn update_tool_activity(&mut self, details: &Value) {
        merge_tool_activity(&mut self.todos, &mut self.agents, details);
        self.save_activity();
    }
    pub(in crate::desktop::workspace) fn save_activity(&mut self) {
        if let Some(ix) = self.active {
            let session = &mut self.db.sessions[ix];
            if session.todos != self.todos || session.agents != self.agents {
                session.todos.clone_from(&self.todos);
                session.agents.clone_from(&self.agents);
                self.dirty = true;
            }
        }
    }
    pub(in crate::desktop::workspace) fn on_subagent(&mut self, payload: &Value) {
        merge_agent(&mut self.agents, payload);
        self.save_activity();
    }
    pub(in crate::desktop::workspace) fn open_subagent(&mut self, id: String) {
        self.collaboration = Default::default();
        self.collaboration.selected = Some(id);
        self.read_subagent();
    }
    fn read_subagent(&mut self) {
        let Some(agent_id) = self.collaboration.selected.clone() else {
            return;
        };
        let Some(s) = self.session() else {
            return;
        };
        let Some(primary) = s.omp_session.as_deref() else {
            return;
        };
        let artifacts = PathBuf::from(primary).with_extension("");
        let path = self
            .agents
            .iter()
            .find(|a| a["id"] == agent_id)
            .and_then(|a| a["sessionFile"].as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| artifacts.join(format!("{agent_id}.jsonl")));
        let session = s.id.clone();
        let tx = self.collaboration.tx.clone();
        let request = id();
        self.collaboration.request = request.clone();
        self.collaboration.loading = true;
        self.collaboration.last_read = Instant::now();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                anyhow::ensure!(
                    crate::infrastructure::omp::sessions::plain_path(&path)
                        .starts_with(crate::infrastructure::omp::sessions::plain_path(&artifacts)),
                    "子会话不属于当前主会话"
                );
                let base = std::fs::canonicalize(artifacts)?;
                let path = std::fs::canonicalize(path)?;
                anyhow::ensure!(path.starts_with(base), "子会话不属于当前主会话");
                SessionStore::transcript(&path)
            })()
            .map_err(|e| format!("暂无法读取子会话：{e:#}"));
            let _ = tx.send(TranscriptResult {
                session,
                agent: agent_id,
                request,
                result,
            });
        });
    }
    pub(in crate::desktop::workspace) fn poll_collaboration(&mut self, cx: &mut Context<Self>) {
        let results: Vec<_> = self.collaboration.rx.try_iter().collect();
        for result in results {
            if self.session().is_none_or(|s| s.id != result.session)
                || self.collaboration.selected.as_ref() != Some(&result.agent)
                || self.collaboration.request != result.request
            {
                continue;
            }
            self.collaboration.loading = false;
            match result.result {
                Ok(messages) => {
                    self.collaboration.messages = messages;
                    self.collaboration.error = None;
                }
                Err(error) => self.collaboration.error = Some(error),
            }
            cx.notify();
        }
        if self.panel_open
            && self.panel_tab == "子智能体"
            && self.collaboration.selected.is_some()
            && !self.collaboration.loading
            && self.collaboration.last_read.elapsed() > Duration::from_secs(2)
        {
            self.read_subagent();
        }
    }
}

use super::*;
use crate::domain::activity::{merge_agent, merge_tool_activity};

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
        merge_tool_activity(&mut self.runtime.todos, &mut self.runtime.agents, details);
        self.save_activity();
    }
    pub(in crate::desktop::workspace) fn save_activity(&mut self) {
        if let Some(ix) = self.active {
            let session = &mut self.db.sessions[ix];
            if session.todos != self.runtime.todos || session.agents != self.runtime.agents {
                session.todos.clone_from(&self.runtime.todos);
                session.agents.clone_from(&self.runtime.agents);
                self.dirty = true;
            }
        }
    }
    pub(in crate::desktop::workspace) fn on_subagent(&mut self, payload: &Value) {
        merge_agent(&mut self.runtime.agents, payload);
        self.save_activity();
    }
    pub(in crate::desktop::workspace) fn open_subagent(&mut self, id: String) {
        self.runtime.collaboration = Default::default();
        self.runtime.collaboration.selected = Some(id);
        self.read_subagent();
    }
    fn read_subagent(&mut self) {
        let Some(agent_id) = self.runtime.collaboration.selected.clone() else {
            return;
        };
        let Some(s) = self.session() else {
            return;
        };
        let Some(primary) = s.omp_session.as_deref() else {
            return;
        };
        let primary = primary.to_owned();
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(_) => return,
        };
        let path = self
            .runtime
            .agents
            .iter()
            .find(|a| a["id"] == agent_id)
            .and_then(|a| a["sessionFile"].as_str())
            .map(str::to_owned);
        let session = s.id.clone();
        let tx = self.runtime.collaboration.tx.clone();
        let request = id();
        self.runtime.collaboration.request = request.clone();
        self.runtime.collaboration.loading = true;
        self.runtime.collaboration.last_read = Instant::now();
        std::thread::spawn(move || {
            let result = host
                .child_transcript(&primary, &agent_id, path.as_deref())
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
        let results: Vec<_> = self.runtime.collaboration.rx.try_iter().collect();
        for result in results {
            if self.session().is_none_or(|s| s.id != result.session)
                || self.runtime.collaboration.selected.as_ref() != Some(&result.agent)
                || self.runtime.collaboration.request != result.request
            {
                continue;
            }
            self.runtime.collaboration.loading = false;
            match result.result {
                Ok(messages) => {
                    self.runtime.collaboration.messages = messages;
                    self.runtime.collaboration.error = None;
                }
                Err(error) => self.runtime.collaboration.error = Some(error),
            }
            cx.notify();
        }
        if self.panel_open
            && self.panel_tab == "子智能体"
            && self.runtime.collaboration.selected.is_some()
            && !self.runtime.collaboration.loading
            && self.runtime.collaboration.last_read.elapsed() > Duration::from_secs(2)
        {
            self.read_subagent();
        }
    }
}

use super::*;
use crate::domain::activity::{merge_agent, merge_tool_activity};

struct TranscriptResult {
    session: String,
    agent: String,
    request: String,
    result: Result<Vec<Message>, String>,
}
pub(in crate::desktop::workspace) struct CollaborationState {
    pub tabs: Vec<String>,
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
            tabs: vec![],
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
            let session = &self.db.sessions[ix];
            if session.todos != self.runtime.todos || session.agents != self.runtime.agents {
                let session = &mut self.db.sessions[ix];
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
        if id.is_empty() || !self.runtime.agents.iter().any(|a| a["id"] == id) {
            return;
        }
        if !self.runtime.collaboration.tabs.contains(&id) {
            self.runtime.collaboration.tabs.push(id.clone());
        }
        self.activate_document(Some(subagent_tab_key(&id)));
    }
    pub(in crate::desktop::workspace) fn active_subagent(&self) -> Option<&str> {
        let id = self
            .document_state
            .active
            .as_deref()?
            .strip_prefix("subagent:")?;
        self.runtime
            .collaboration
            .tabs
            .iter()
            .find(|tab| tab.as_str() == id)
            .map(String::as_str)
    }
    pub(in crate::desktop::workspace) fn select_subagent(&mut self, id: String) {
        if self.runtime.collaboration.selected.as_ref() == Some(&id) {
            return;
        }
        self.runtime.collaboration.messages.clear();
        self.runtime.collaboration.error = None;
        self.runtime.collaboration.loading = false;
        self.runtime.collaboration.selected = Some(id);
        self.read_subagent();
    }
    pub(in crate::desktop::workspace) fn close_subagent_tab(&mut self, key: &str) -> bool {
        let Some(ix) = self
            .runtime
            .collaboration
            .tabs
            .iter()
            .position(|id| subagent_tab_key(id) == key)
        else {
            return false;
        };
        let id = self.runtime.collaboration.tabs.remove(ix);
        if self.runtime.collaboration.selected.as_ref() == Some(&id) {
            self.runtime.collaboration.selected = None;
            self.runtime.collaboration.messages.clear();
            self.runtime.collaboration.loading = false;
            self.runtime.collaboration.error = None;
        }
        if self.document_state.active.as_deref() == Some(key) {
            let next = ix
                .checked_sub(1)
                .and_then(|ix| self.runtime.collaboration.tabs.get(ix))
                .or_else(|| self.runtime.collaboration.tabs.first())
                .map(|id| subagent_tab_key(id))
                .or_else(|| self.document_state.tabs.last().map(|tab| tab.key.clone()));
            self.activate_document(next);
        }
        true
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
        if !self.polling_background
            && self.active_subagent().is_some()
            && !self.runtime.collaboration.loading
            && self.runtime.collaboration.last_read.elapsed() > Duration::from_secs(2)
        {
            self.read_subagent();
        }
    }
}

pub(in crate::desktop::workspace) fn subagent_tab_key(id: &str) -> String {
    format!("subagent:{id}")
}

pub(in crate::desktop::workspace) fn subagent_role(agent: &Value) -> &str {
    ["role", "agent", "name"]
        .into_iter()
        .filter_map(|key| agent[key].as_str())
        .find(|text| !text.trim().is_empty())
        .unwrap_or("子智能体")
}

pub(in crate::desktop::workspace) fn subagent_title(agent: &Value) -> &str {
    ["title", "description", "task", "assignment", "name"]
        .into_iter()
        .filter_map(|key| agent[key].as_str())
        .find(|text| !text.trim().is_empty())
        .unwrap_or("子会话")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::workspace::runtime::sessions::tests::fixture;
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn stale_transcripts_cannot_replace_another_tab_or_reopen_a_closed_tab(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| fixture(window, cx))
            })
            .unwrap()
        });
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.runtime.agents = vec![json!({"id":"a"}), json!({"id":"b"})];
                    this.open_subagent("a".into());
                    this.runtime.collaboration.request = "read-a".into();
                    let session = this.session().unwrap().id.clone();
                    this.open_subagent("b".into());
                    this.runtime.collaboration.request = "read-b".into();
                    for (agent, request, text) in
                        [("a", "read-a", "stale A"), ("b", "read-b", "current B")]
                    {
                        this.runtime
                            .collaboration
                            .tx
                            .send(TranscriptResult {
                                session: session.clone(),
                                agent: agent.into(),
                                request: request.into(),
                                result: Ok(vec![Message::new("assistant", text.into())]),
                            })
                            .unwrap();
                    }
                    this.panel_open = false;
                    this.poll_collaboration(cx);
                    assert_eq!(this.runtime.collaboration.messages[0].text, "current B");
                    this.close_document("subagent:b");
                    assert_eq!(this.active_subagent(), Some("a"));
                    this.runtime
                        .collaboration
                        .tx
                        .send(TranscriptResult {
                            session,
                            agent: "b".into(),
                            request: "read-b".into(),
                            result: Ok(vec![]),
                        })
                        .unwrap();
                    this.poll_collaboration(cx);
                    assert!(this.runtime.collaboration.messages.is_empty());
                    assert_eq!(this.runtime.collaboration.tabs, vec!["a"]);
                })
            })
            .unwrap();
    }
}

use super::*;

impl Workspace {
    pub(super) fn update_working_intent(&mut self, frame: &Value) {
        match frame["type"].as_str().unwrap_or_default() {
            "agent_start" => self.runtime.working_intent = None,
            "session_settled" | "prompt_result" if frame["sessionSettled"] != false => {
                self.runtime.working_intent = None;
            }
            "response"
                if frame["command"] == "get_state"
                    && frame["success"] == true
                    && frame["data"]["isSettled"] == true
                    && self.runtime.pending_send.is_none() =>
            {
                self.runtime.working_intent = None;
            }
            _ if self.runtime.running && !self.runtime.stopping => {
                if let Some(intent) = crate::infrastructure::omp::working_intent(frame) {
                    self.runtime.working_intent = Some(intent);
                }
            }
            _ => {}
        }
    }
}

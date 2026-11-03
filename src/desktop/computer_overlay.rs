//! Ephemeral local desktop indication. No images, input text, or history are saved.
use serde_json::Value;

#[cfg(windows)]
mod windows;

#[derive(Default)]
pub(crate) struct Activity {
    #[cfg(all(windows, not(test)))]
    owner: Option<String>,
    active: bool,
    suspended: bool,
    finished: bool,
    waiting: std::collections::HashSet<String>,
}

impl Activity {
    pub fn on_frame(&mut self, frame: &Value, local: bool) {
        if !local {
            self.clear();
            return;
        }
        let kind = frame["type"].as_str().unwrap_or("");
        if kind == "agent_start" || kind == "ready" {
            self.clear();
            self.finished = false;
            return;
        }
        if self.finished {
            return;
        }
        match kind {
            "session_settled" | "agent_end" => self.stop(),
            "prompt_result" if frame["sessionSettled"] != false => self.stop(),
            "extension_ui_request"
                if matches!(
                    frame["method"].as_str(),
                    Some("select" | "confirm" | "input" | "editor")
                ) =>
            {
                self.waiting
                    .insert(frame["id"].as_str().unwrap_or("").to_owned());
                self.suspended = true;
                self.publish(None);
            }
            "extension_ui_request" if frame["method"] == "cancel" => {
                self.waiting
                    .remove(frame["targetId"].as_str().unwrap_or(""));
                self.suspended = !self.waiting.is_empty();
                self.publish(None);
            }
            "response" if frame["command"] == "get_state" && frame["data"]["isSettled"] == true => {
                self.stop()
            }
            "tool_execution_start" => {
                let name = frame["toolName"].as_str().unwrap_or("");
                // Eval handles can survive between calls. Once a computer call is
                // observed, keep indicating the turn, including subsequent cells.
                if name == "computer" || (name == "eval" && mentions_computer(&frame["args"])) {
                    self.active = true;
                }
                self.publish(None);
            }
            "tool_execution_update" | "tool_execution_end" => {
                let result = frame.get("partialResult").or_else(|| frame.get("result"));
                if let Some(result) = result {
                    let details = &result["details"];
                    let last = details["statusEvents"]
                        .as_array()
                        .and_then(|events| {
                            events.iter().rev().find(|event| event["op"] == "computer")
                        })
                        .or_else(|| {
                            details["cells"].as_array().and_then(|cells| {
                                cells
                                    .iter()
                                    .rev()
                                    .flat_map(|cell| {
                                        cell["statusEvents"].as_array().into_iter().flatten().rev()
                                    })
                                    .find(|event| event["op"] == "computer")
                            })
                        });
                    if let Some(event) = last {
                        self.active = true;
                        // Only show a short status, never the script or typed text.
                        self.publish(Some(if event["error"].is_string() {
                            "操作未完成，请查看会话"
                        } else {
                            crate::domain::computer_operation_label(
                                event["detail"].as_str().unwrap_or(""),
                            )
                        }));
                    }
                }
            }
            "__omp_gui_transport" if frame["state"] != "connected" => self.clear(),
            _ => {}
        }
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.suspended = false;
        self.waiting.clear();
        self.publish(None);
    }

    pub fn stop(&mut self) {
        self.clear();
        self.finished = true;
    }

    pub fn resume(&mut self) {
        self.waiting.clear();
        self.suspended = false;
        self.publish(None);
    }

    fn publish(&mut self, status: Option<&str>) {
        #[cfg(all(windows, not(test)))]
        {
            let visible = self.active && !self.suspended;
            if visible || self.owner.is_some() {
                let owner = self
                    .owner
                    .get_or_insert_with(|| uuid::Uuid::new_v4().to_string());
                windows::update(owner, visible, status);
            }
        }
        #[cfg(any(not(windows), test))]
        let _ = status;
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        self.clear();
    }
}

fn mentions_computer(args: &Value) -> bool {
    args["code"].as_str().is_some_and(|code| {
        code.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .any(|word| word == "computer")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn activity_is_local_live_and_clears_on_completion_or_disconnect() {
        let mut activity = Activity::default();
        let start = json!({"type":"tool_execution_start","toolName":"eval","args":{"code":"await computer.window(1)"}});
        activity.on_frame(&start, false);
        assert!(!activity.active);
        activity.on_frame(&start, true);
        assert!(activity.active);
        activity.on_frame(
            &json!({"type":"extension_ui_request","method":"confirm"}),
            true,
        );
        assert!(activity.suspended);
        activity.on_frame(&json!({"type":"tool_execution_update","partialResult":{"details":{"statusEvents":[{"op":"computer","detail":"desktop.screenshot()"}]}}}), true);
        assert!(
            activity.suspended,
            "cumulative progress must not reopen the overlay during a question"
        );
        activity.resume();
        activity.on_frame(&json!({"type":"tool_execution_start","toolName":"eval","args":{"code":"await win.click(1,2)"}}), true);
        assert!(activity.active && !activity.suspended);
        activity.on_frame(&json!({"type":"session_settled"}), true);
        assert!(!activity.active);
        let update = json!({"type":"tool_execution_update","partialResult":{"details":{"statusEvents":[{"op":"computer","detail":"desktop.click(1,2)"}]}}});
        activity.on_frame(&update, true);
        assert!(
            !activity.active,
            "late progress must not revive a completed turn"
        );
        activity.on_frame(&json!({"type":"agent_start"}), true);
        activity.on_frame(&update, true);
        assert!(activity.active);
        activity.on_frame(
            &json!({"type":"__omp_gui_transport","state":"disconnected"}),
            true,
        );
        assert!(!activity.active);
    }
}

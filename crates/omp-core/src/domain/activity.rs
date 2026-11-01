//! Session-owned task and agent records; terminal roster snapshots omit finished peers.
use serde_json::{Value, json};
/// get_state may omit optional presentation fields retained in the journal.
pub fn merge_todo_snapshot(previous: &[Value], phases: &[Value]) -> Vec<Value> {
    let mut phases = phases.to_vec();
    for phase in &mut phases {
        let Some(old) = previous.iter().find(|p| p["name"] == phase["name"]) else {
            continue;
        };
        for task in phase["tasks"].as_array_mut().into_iter().flatten() {
            if let Some(prior) = old["tasks"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["content"] == task["content"])
            {
                for key in ["details", "notes"] {
                    if task.get(key).is_none()
                        && let Some(value) = prior.get(key)
                    {
                        task[key] = value.clone();
                    }
                }
            }
        }
    }
    phases
}
pub fn merge_agent(agents: &mut Vec<Value>, update: &Value) {
    let id = update["id"]
        .as_str()
        .or_else(|| update["progress"]["id"].as_str());
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return;
    };
    let index = agents
        .iter()
        .position(|a| a["id"] == id)
        .unwrap_or_else(|| {
            agents.push(json!({"id":id}));
            agents.len() - 1
        });
    let terminal = matches!(
        agents[index]["status"].as_str(),
        Some("completed" | "failed" | "aborted")
    );
    let incoming = update["status"]
        .as_str()
        .or_else(|| update["progress"]["status"].as_str());
    if terminal && matches!(incoming, Some("running" | "pending")) {
        return;
    }
    if incoming == Some("started") {
        agents[index]["output"] = Value::Null;
        agents[index]["error"] = Value::Null;
    }
    if let Some(fields) = update.as_object() {
        for (key, value) in fields {
            if !value.is_null() {
                agents[index][key] = value.clone();
            }
        }
    }
    if let Some(fields) = update["progress"].as_object() {
        for key in [
            "agent",
            "task",
            "assignment",
            "description",
            "status",
            "output",
            "sessionFile",
        ] {
            if let Some(value) = fields.get(key) {
                agents[index][key] = value.clone();
            }
        }
    }
    if agents[index]["status"] == "started" {
        agents[index]["status"] = json!("running");
    }
    if update.get("exitCode").is_some() {
        agents[index]["status"] = json!(if update["aborted"] == true {
            "aborted"
        } else if update["exitCode"] == 0 {
            "completed"
        } else {
            "failed"
        });
    }
}
pub fn merge_tool_activity(todos: &mut Vec<Value>, agents: &mut Vec<Value>, details: &Value) {
    if let Some(phases) = details["phases"].as_array() {
        *todos = phases.clone();
    }
    for list in ["results", "progress"] {
        for agent in details[list].as_array().into_iter().flatten() {
            merge_agent(agents, agent);
        }
    }
}
pub fn status_label(status: &str) -> &str {
    match status {
        "started" | "running" | "in_progress" => "进行中",
        "completed" => "已完成",
        "blocked" => "受阻",
        "abandoned" => "已放弃",
        "aborted" | "cancelled" => "已停止",
        "failed" | "error" => "失败",
        "pending" | "queued" => "待处理",
        "idle" => "空闲",
        other => other,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finished_agents_keep_their_transcript_and_task_results_update_todos() {
        let mut agents = vec![];
        let mut todos = vec![];
        merge_agent(
            &mut agents,
            &json!({"id":"scout","status":"started","sessionFile":"child.jsonl"}),
        );
        merge_agent(
            &mut agents,
            &json!({"progress":{"id":"scout","agent":"reviewer","status":"running","task":"review"}}),
        );
        merge_tool_activity(
            &mut todos,
            &mut agents,
            &json!({"phases":[{"name":"phase","tasks":[]}],"results":[{"id":"scout","exitCode":0,"output":"done"}]}),
        );
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0]["status"], "completed");
        assert_eq!(agents[0]["sessionFile"], "child.jsonl");
        assert_eq!(agents[0]["task"], "review");
        assert_eq!(todos.len(), 1);
        merge_agent(&mut agents, &json!({"id":"scout","status":"running"}));
        assert_eq!(
            agents[0]["status"], "completed",
            "late roster snapshots must not revive completed agents"
        );
        assert_eq!(status_label("blocked"), "受阻");
    }
    #[test]
    fn state_snapshots_keep_notes_but_do_not_resurrect_removed_tasks() {
        let old = vec![
            json!({"name":"phase","tasks":[{"content":"keep","status":"pending","details":"detail","notes":["note"]},{"content":"remove","status":"pending"}]}),
        ];
        let incoming =
            vec![json!({"name":"phase","tasks":[{"content":"keep","status":"completed"}]})];
        let merged = merge_todo_snapshot(&old, &incoming);
        assert_eq!(merged[0]["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(merged[0]["tasks"][0]["status"], "completed");
        assert_eq!(merged[0]["tasks"][0]["notes"][0], "note");
        assert!(merge_todo_snapshot(&old, &[]).is_empty());
    }
}

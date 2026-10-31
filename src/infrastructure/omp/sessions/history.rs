use super::*;
use crate::domain::{Message, activity::merge_tool_activity};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Ownership/deletion only needs the bounded header, even after a torn append.
pub(super) fn header(path: &Path) -> Result<Value> {
    let mut reader = BufReader::new(fs::File::open(path)?);
    for _ in 0..2 {
        let mut line = String::new();
        std::io::Read::by_ref(&mut reader)
            .take(65536)
            .read_line(&mut line)?;
        if let Ok(header) = serde_json::from_str::<Value>(&line)
            && header["type"] == "session"
        {
            return Ok(header);
        }
    }
    anyhow::bail!("OMP 会话缺少有效文件头");
}

pub(super) fn read(path: &Path) -> Result<NativeSession> {
    let bytes = fs::read(path)?;
    let mut rows = Vec::new();
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        // A writer can have an incomplete final append. Never replace GUI
        // history with that partial snapshot; retry on the next scan.
        rows.push(serde_json::from_slice::<Value>(line)?);
    }
    let header = rows
        .iter()
        .find(|r| r["type"] == "session")
        .context("OMP 会话缺少文件头")?;
    let id = header["id"]
        .as_str()
        .context("OMP 会话缺少编号")?
        .to_owned();
    let cwd = PathBuf::from(header["cwd"].as_str().context("OMP 会话缺少工作目录")?);
    let mut title = header["title"].as_str().unwrap_or_default().to_owned();
    for r in &rows {
        match r["type"].as_str() {
            Some("title" | "title_change") => {
                if let Some(t) = r["title"].as_str() {
                    title = t.into();
                }
            }
            Some("session_info") => {
                if let Some(t) = r["name"].as_str() {
                    title = t.into();
                }
            }
            _ => {}
        }
    }
    // The fixed title slot is authoritative over legacy metadata entries.
    if let Some(slot) = rows.first().filter(|r| r["type"] == "title") {
        title = slot["title"].as_str().unwrap_or_default().into();
    }
    let entries: HashMap<&str, &Value> = rows
        .iter()
        .filter(|r| r["type"] != "session")
        .filter_map(|r| Some((r["id"].as_str()?, r)))
        .collect();
    let mut leaf = rows
        .iter()
        .rev()
        .find(|r| r["id"].is_string() && r["type"] != "session");
    if let Some(last) =
        leaf.filter(|r| r["type"] == "model_usage" && r.get("activeLeafId").is_some())
    {
        leaf = if last["activeLeafId"].is_null() {
            None
        } else {
            last["activeLeafId"]
                .as_str()
                .and_then(|id| entries.get(id).copied())
                .or_else(|| {
                    rows.iter().rev().find(|r| {
                        r["type"] != "session"
                            && r["id"].is_string()
                            && (r["type"] != "model_usage"
                                || r.get("activeLeafId").is_none()
                                || r["activeLeafId"] == r["id"])
                    })
                })
        };
    }
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    while let Some(row) = leaf {
        let key = row["id"].as_str().unwrap_or_default();
        if !seen.insert(key) {
            break;
        }
        chain.push(row);
        leaf = row["parentId"]
            .as_str()
            .and_then(|p| entries.get(p).copied());
    }
    chain.reverse();
    let mut messages: Vec<Message> = Vec::new();
    let mut todos = Vec::new();
    let mut agents = Vec::new();
    for row in chain {
        if row["type"] == "reset_boundary" {
            messages.clear();
            todos.clear();
            agents.clear();
        }
        if row["type"] == "custom"
            && row["customType"] == "user_todo_edit"
            && let Some(phases) = row["data"]["phases"].as_array()
        {
            todos = phases.clone();
        }
        let custom;
        let m = if row["type"] == "custom_message" {
            custom = serde_json::json!({"role":"custom","customType":row["customType"],"content":row["content"],"display":row["display"],"details":row["details"],"timestamp":row["timestamp"]});
            &custom
        } else if row["type"] == "message" {
            &row["message"]
        } else {
            continue;
        };
        if m["role"] == "toolResult" {
            merge_tool_activity(&mut todos, &mut agents, &m["details"]);
            let call = m["toolCallId"].as_str().unwrap_or_default();
            if let Some(block) = messages
                .iter_mut()
                .rev()
                .flat_map(|m| m.process.iter_mut())
                .find(|b| b.id == call)
            {
                block.set_result(super::super::message_text(m), &m["details"]);
            }
        } else if let Some(mut message) = super::super::display_message(m) {
            message.native_id = row["id"].as_str().map(str::to_owned);
            message.id = format!(
                "native-{}",
                message.native_id.as_deref().unwrap_or_default()
            );
            message.time = m["timestamp"]
                .as_u64()
                .map(|ms| ms / 1000)
                .or_else(|| {
                    row["timestamp"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.timestamp().max(0) as u64)
                })
                .unwrap_or(0);
            messages.push(message);
        }
    }
    if title.trim().is_empty() {
        title = messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.text.chars().take(48).collect())
            .unwrap_or_default();
    }
    let metadata = fs::metadata(path)?;
    let updated = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(NativeSession {
        id,
        path: plain_path(path),
        cwd,
        title,
        updated,
        messages: Arc::new(messages),
        todos,
        agents,
        revision: format!("display-v2:{:x}", Sha256::digest(&bytes)),
    })
}

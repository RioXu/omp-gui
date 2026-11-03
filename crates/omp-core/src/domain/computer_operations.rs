//! Small presentation records from OMP's Eval status log, shared by live/history paths.
use serde_json::Value;

const MAX_OPERATIONS: usize = 128;
const MAX_TEXT: usize = 240;

pub(super) fn project(details: &Value) -> Option<Vec<String>> {
    // The top-level log already combines cells; reading both would duplicate calls.
    let events: Vec<&Value> = if let Some(events) = details["statusEvents"].as_array() {
        events.iter().collect()
    } else {
        let cells = details["cells"].as_array()?;
        cells
            .iter()
            .flat_map(|cell| cell["statusEvents"].as_array().into_iter().flatten())
            .collect()
    };
    let computer: Vec<_> = events
        .into_iter()
        .filter(|event| event["op"] == "computer")
        .collect();
    let hidden = computer.len().saturating_sub(MAX_OPERATIONS);
    let mut lines = Vec::new();
    if hidden > 0 {
        lines.push(format!("另有 {hidden} 项较早电脑操作未保留"));
    }
    for event in computer.into_iter().skip(hidden) {
        if let Some(error) = event["error"].as_str().filter(|s| !s.is_empty()) {
            lines.push(format!("电脑操作失败 · {}", short_text(error)));
        } else {
            let detail = event["detail"].as_str().unwrap_or("");
            let label = operation_label(detail);
            lines.push(if detail.is_empty() {
                label.into()
            } else {
                format!("{label} · {}", short_text(detail))
            });
        }
    }
    Some(lines)
}

fn short_text(text: &str) -> String {
    let mut chars = text.chars();
    let mut result: String = chars
        .by_ref()
        .take(MAX_TEXT)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

pub fn operation_label(detail: &str) -> &'static str {
    // Only inspect method calls at depth zero in OMP's rendered chain. A user's
    // typed text containing '.click(' must not turn a keyboard action into a click.
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut start = 0;
    let mut method = "";
    for (index, ch) in detail.char_indices().take(4096) {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => {
                if depth == 0 {
                    method = &detail[start..index];
                }
                depth += 1;
            }
            ')' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => start = index + 1,
            _ => {}
        }
    }
    match method {
        "screenshot" => "截图",
        "zoom" => "局部截图",
        "click" => "点击",
        "doubleClick" => "双击",
        "move" => "移动鼠标",
        "drag" => "拖动",
        "scroll" => "滚动",
        "type" | "setValue" => "输入文字",
        "press" | "holdKeys" => "按键／控件操作",
        "holdMouse" => "按住鼠标",
        "ax" | "find" => "检查界面元素",
        "raise" | "focus" => "聚焦窗口／控件",
        "run" => "执行桌面脚本",
        _ => "电脑操作",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_canonical_calls_without_copying_images_or_duplicate_cells() {
        let events = json!([
            {"op":"computer","detail":"desktop.window({\"app\":\"记事本\"}).screenshot()"},
            {"op":"computer","detail":"desktop.type(\"text.click(1, 2)\")"},
            {"op":"computer","error":"InputBusy"},
            {"op":"browser","detail":"other action"}
        ]);
        let details = json!({"statusEvents":events,"cells":[{"statusEvents":events}],
            "images":[{"data":"large image payload"}]});
        let lines = project(&details).unwrap();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("截图 ·"));
        assert!(lines[1].starts_with("输入文字 ·"));
        assert_eq!(lines[2], "电脑操作失败 · InputBusy");
        assert!(!serde_json::to_string(&lines).unwrap().contains("payload"));
        assert_eq!(
            project(&json!({"cells":[{"statusEvents":events}]})),
            Some(lines)
        );
    }

    #[test]
    fn bounds_long_logs_and_unicode_text() {
        let events: Vec<_> = (0..200)
            .map(|_| json!({"op":"computer","detail":"中".repeat(1000)}))
            .collect();
        let lines = project(&json!({"statusEvents":events})).unwrap();
        assert_eq!(lines.len(), MAX_OPERATIONS + 1);
        assert!(lines[0].contains("72"));
        assert!(lines[1].ends_with('…'));
        assert!(serde_json::to_vec(&lines).unwrap().len() < 100_000);
    }

    #[test]
    fn cumulative_updates_replace_records_and_old_workspaces_remain_readable() {
        let mut block = crate::domain::process_blocks(&json!({"content":[
            {"type":"toolCall","id":"eval-1","name":"eval","arguments":{"code":"await computer.screenshot()"}}
        ]}), "message_end", "").remove(0);
        let snapshot = json!({"statusEvents":[{"op":"computer","detail":"desktop.screenshot()"}]});
        block.set_result("text".into(), &snapshot);
        block.set_result("text".into(), &snapshot);
        assert_eq!(block.computer_operations.len(), 1);
        let mut saved = serde_json::to_value(&block).unwrap();
        let restored: crate::domain::ProcessBlock = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(restored.computer_operations, block.computer_operations);
        saved.as_object_mut().unwrap().remove("computer_operations");
        let legacy: crate::domain::ProcessBlock = serde_json::from_value(saved).unwrap();
        assert!(legacy.computer_operations.is_empty());
    }
}

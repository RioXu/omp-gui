//! Display-only notice formatting. Native text and journals remain authoritative.
use super::Message;
use serde_json::Value;
use std::borrow::Cow;

pub fn display_body(message: &Message) -> Cow<'_, str> {
    if message.role != "custom" {
        return Cow::Borrowed(&message.text);
    }
    let body = unwrap_notice(&message.text);
    if message
        .label
        .as_deref()
        .is_some_and(|s| s.starts_with("lsp-"))
    {
        let files = message.details["files"].as_array();
        let formatted = files.and_then(|files| {
            files
                .iter()
                .map(format_file)
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.join("\n\n"))
        });
        if let Some(formatted) = formatted.filter(|s| !s.is_empty()) {
            return Cow::Owned(formatted);
        }
        // Older services/cached histories may have only the textual envelope.
        let mut formatted = Vec::new();
        for line in body.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if line == "Late LSP diagnostics arrived after the edit returned:" {
                continue;
            }
            if let Some(diagnostic) = diagnostic(line, None) {
                formatted.push(diagnostic);
            } else if let Some((path, summary)) = line.split_once(" — ") {
                formatted.push(format!("**{}** · {}", inline(path), summary_text(summary)));
            } else {
                formatted.push(literal(line));
            }
        }
        if !formatted.is_empty() {
            return Cow::Owned(formatted.join("\n\n"));
        }
    }
    Cow::Borrowed(body)
}

fn unwrap_notice(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix("<system-notice>")
        .and_then(|s| s.strip_suffix("</system-notice>"))
        .map(str::trim)
        .unwrap_or(text)
}

fn format_file(file: &Value) -> Option<String> {
    let path = file["path"].as_str()?;
    let mut output = format!("**{}**", inline(path));
    if let Some(summary) = file["summary"].as_str().filter(|s| !s.is_empty()) {
        output.push_str(&format!(" · {}", summary_text(summary)));
    }
    for message in file["messages"].as_array().into_iter().flatten() {
        let text = message
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| message.to_string());
        output.push_str("\n\n");
        output.push_str(
            &diagnostic(&text, Some(path)).unwrap_or_else(|| format!("- {}", literal(&text))),
        );
    }
    Some(output)
}

fn diagnostic(line: &str, file: Option<&str>) -> Option<String> {
    let (location, rest) = line.split_once(" [")?;
    let (path_line, column) = location.rsplit_once(':')?;
    let (path, row) = path_line.rsplit_once(':')?;
    row.parse::<u64>().ok()?;
    column.parse::<u64>().ok()?;
    let (severity, mut text) = rest.split_once(']')?;
    text = text.trim();
    let mut source = "";
    if let Some(rest) = text.strip_prefix('[')
        && let Some((name, rest)) = rest.split_once(']')
    {
        source = name;
        text = rest.trim();
    }
    let severity = match severity {
        "error" => "错误",
        "warning" | "warn" => "警告",
        "information" | "info" | "note" => "信息",
        "hint" => "提示",
        other => other,
    };
    let same_file = file.is_some_and(|file| {
        file == path
            || file
                .strip_suffix(path)
                .is_some_and(|prefix| prefix.ends_with(['/', '\\']))
    });
    let location = if same_file {
        format!("{row}:{column}")
    } else {
        format!("{path}:{row}:{column}")
    };
    let mut output = format!("- **{}** · {}", literal(severity), inline(&location));
    if !source.is_empty() {
        output.push_str(&format!(" · {}", literal(source)));
    }
    output.push_str("  \n  ");
    output.push_str(&literal(text));
    Some(output)
}

fn summary_text(summary: &str) -> String {
    summary
        .split(',')
        .map(|part| {
            let part = part.trim();
            let Some((count, kind)) = part.split_once(' ') else {
                return literal(part);
            };
            if count.parse::<u64>().is_err() {
                return literal(part);
            }
            match kind {
                "error(s)" | "errors" | "error" => format!("{count} 个错误"),
                "warning(s)" | "warnings" | "warning" => format!("{count} 个警告"),
                "hint(s)" | "hints" | "hint" => format!("{count} 个提示"),
                _ => literal(part),
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}
fn literal(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' => {
                result.push('\\');
                result.push(c);
            }
            '\n' => result.push_str("\n  "),
            _ => result.push(c),
        }
    }
    result
}
fn inline(text: &str) -> String {
    let fence = "`".repeat(text.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
    format!("{fence} {} {fence}", text.replace(['\n', '\r'], " "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn structured_lsp_is_readable_without_changing_native_text() {
        let mut message = Message::new(
            "custom",
            "<system-notice>raw diagnostic</system-notice>".into(),
        );
        message.label = Some("lsp-late-diagnostic".into());
        message.details = json!({"files":[{"path":"src/中文 file.cpp", "summary":"2 error(s), 1 warning(s)", "messages":[
            "src/中文 file.cpp:52:35 [error] [clang] No type named '_Tp' in std::vector<A<B>> (unknown_type)",
            "C:\\src\\a.cpp:18:1 [warning] [clangd] Header is not used (unused-includes)",
            "Unexpected diagnostic without a location"
        ]}]});
        let body = display_body(&message);
        assert!(body.contains("src/中文 file.cpp"));
        assert!(body.contains("**错误** · ` 52:35 ` · clang"));
        assert!(body.contains("**警告** · ` C:\\src\\a.cpp:18:1 ` · clangd"));
        assert!(body.contains("std::vector&lt;A&lt;B&gt;&gt;"));
        assert!(body.contains("Unexpected diagnostic without a location"));
        assert!(!body.contains("<system-notice>"));
        assert_eq!(
            message.text,
            "<system-notice>raw diagnostic</system-notice>"
        );
    }
    #[test]
    fn old_lsp_text_and_unknown_notices_keep_all_content() {
        let mut message = Message::new("custom", "<system-notice>\nLate LSP diagnostics arrived after the edit returned:\na.cpp — 1 error(s)\na.cpp:1:2 [error] [clang] Invalid <T>\nAdditional context\n</system-notice>".into());
        message.label = Some("lsp-late-diagnostic".into());
        let text = display_body(&message);
        assert!(text.contains("**错误**"));
        assert!(text.contains("Additional context"));
        assert!(text.contains("Invalid &lt;T&gt;"));
        message.label = Some("async-result".into());
        assert!(!display_body(&message).contains("<system-notice>"));
        message.role = "assistant".into();
        assert_eq!(display_body(&message), message.text);
    }
}

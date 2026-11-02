//! Language-independent LSP/OMP diagnostic normalization for native presentation.
use crate::domain::{Message, fenced};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
    Unknown,
}
impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Error => "错误",
            Self::Warning => "警告",
            Self::Information => "信息",
            Self::Hint => "提示",
            Self::Unknown => "诊断",
        }
    }
    fn parse(value: &Value) -> Self {
        match value.as_str().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("error" | "1") => Self::Error,
            Some("warning" | "warn" | "2") => Self::Warning,
            Some("information" | "info" | "note" | "3") => Self::Information,
            Some("hint" | "4") => Self::Hint,
            _ => match value.as_u64() {
                Some(1) => Self::Error,
                Some(2) => Self::Warning,
                Some(3) => Self::Information,
                Some(4) => Self::Hint,
                _ => Self::Unknown,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub path: String,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub source: String,
    pub code: String,
    pub message: String,
    pub related: Vec<String>,
}
impl Diagnostic {
    pub fn location(&self) -> String {
        match (self.line, self.column) {
            (Some(line), Some(column)) => format!("{line}:{column}"),
            (Some(line), None) => line.to_string(),
            _ => String::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Block {
    File { path: String, summary: String },
    Diagnostic(Diagnostic),
    Text { text: String },
}
impl Block {
    pub fn text(&self) -> String {
        match self {
            Self::File { path, summary } => format!("{path}\n{summary}"),
            Self::Diagnostic(d) => {
                let metadata = [
                    d.severity.label().to_owned(),
                    d.path.clone(),
                    d.location(),
                    d.source.clone(),
                    d.code.clone(),
                ]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("  ");
                let mut text = format!("{metadata}\n{}", d.message);
                for related in &d.related {
                    text.push('\n');
                    text.push_str(related);
                }
                text
            }
            Self::Text { text } => text.clone(),
        }
    }
}
pub fn is_lsp(message: &Message) -> bool {
    message.role == "custom"
        && message
            .label
            .as_deref()
            .is_some_and(|s| s == "lsp" || s.starts_with("lsp-"))
}
pub fn blocks(message: &Message) -> Vec<Block> {
    let mut blocks = Vec::new();
    if let Some(files) = message.details["files"].as_array() {
        for file in files {
            let Some(path) = file["path"].as_str().or_else(|| file["uri"].as_str()) else {
                blocks.push(Block::Text {
                    text: file.to_string(),
                });
                continue;
            };
            append_file(&mut blocks, path, file);
        }
    } else if message.details["diagnostics"].is_array() {
        let path = message.details["uri"]
            .as_str()
            .or_else(|| message.details["path"].as_str())
            .unwrap_or_default();
        append_file(&mut blocks, path, &message.details);
    }
    if !blocks.is_empty() {
        return blocks;
    }
    let mut current = String::new();
    for line in super::unwrap_notice(&message.text).lines() {
        if line.trim() == "Late LSP diagnostics arrived after the edit returned:" {
            continue;
        }
        if let Some(d) = parse_text(line, &current) {
            blocks.push(Block::Diagnostic(d));
        } else if let Some((path, summary)) = line.split_once(" — ") {
            current = path.trim().into();
            blocks.push(Block::File {
                path: current.clone(),
                summary: super::summary_text(summary),
            });
        } else if !line.trim().is_empty() {
            // Continuations from Python/TypeScript keep their line breaks.
            if (line.starts_with(char::is_whitespace) || line.trim_start().starts_with('→'))
                && let Some(Block::Diagnostic(d)) = blocks.last_mut()
            {
                d.message.push('\n');
                d.message.push_str(line);
            } else {
                blocks.push(Block::Text { text: line.into() });
            }
        }
    }
    blocks
}
fn append_file(blocks: &mut Vec<Block>, path: &str, file: &Value) {
    let path = file_path(path);
    let values = file["messages"]
        .as_array()
        .or_else(|| file["diagnostics"].as_array());
    let diagnostics = values
        .into_iter()
        .flatten()
        .map(|value| parse_value(value, &path))
        .collect::<Vec<_>>();
    let summary = file["summary"]
        .as_str()
        .map(super::summary_text)
        .unwrap_or_else(|| {
            [
                Severity::Error,
                Severity::Warning,
                Severity::Information,
                Severity::Hint,
            ]
            .into_iter()
            .filter_map(|severity| {
                let count = diagnostics
                    .iter()
                    .filter(|d| d.severity == severity)
                    .count();
                (count > 0).then(|| format!("{count} 个{}", severity.label()))
            })
            .collect::<Vec<_>>()
            .join("  ")
        });
    blocks.push(Block::File { path, summary });
    blocks.extend(diagnostics.into_iter().map(Block::Diagnostic));
}
fn parse_value(value: &Value, file: &str) -> Diagnostic {
    if let Some(text) = value.as_str() {
        return parse_text(text, file).unwrap_or_else(|| Diagnostic {
            severity: Severity::Unknown,
            path: String::new(),
            line: None,
            column: None,
            source: String::new(),
            code: String::new(),
            message: text.into(),
            related: Vec::new(),
        });
    }
    let path = value["uri"]
        .as_str()
        .or_else(|| value["path"].as_str())
        .map(file_path)
        .unwrap_or_default();
    let path = if same_file(&path, file) {
        String::new()
    } else {
        path
    };
    let related = value["relatedInformation"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|info| {
            let position = &info["location"]["range"]["start"];
            let path = info["location"]["uri"]
                .as_str()
                .map(file_path)
                .unwrap_or_default();
            format!(
                "{}{}  {}",
                path,
                position["line"]
                    .as_u64()
                    .map(|line| format!(
                        ":{}:{}",
                        line.saturating_add(1),
                        position["character"]
                            .as_u64()
                            .unwrap_or(0)
                            .saturating_add(1)
                    ))
                    .unwrap_or_default(),
                info["message"].as_str().unwrap_or_default()
            )
        })
        .collect();
    Diagnostic {
        severity: Severity::parse(&value["severity"]),
        path,
        line: value["range"]["start"]["line"]
            .as_u64()
            .map(|n| n.saturating_add(1)),
        column: value["range"]["start"]["character"]
            .as_u64()
            .map(|n| n.saturating_add(1)),
        source: value["source"].as_str().unwrap_or_default().into(),
        code: value["code"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| value["code"].as_i64().map(|n| n.to_string()))
            .unwrap_or_default(),
        message: value["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string()),
        related,
    }
}
fn same_file(path: &str, file: &str) -> bool {
    path == file
        || file
            .strip_suffix(path)
            .is_some_and(|prefix| prefix.ends_with('/'))
}
pub fn parse_text(text: &str, file: &str) -> Option<Diagnostic> {
    let (location, rest) = text.match_indices(" [").find_map(|(at, _)| {
        let location = &text[..at];
        let (_, column) = location.rsplit_once(':')?;
        column.parse::<u64>().ok()?;
        Some((location, &text[at + 2..]))
    })?;
    let (path_line, column) = location.rsplit_once(':')?;
    let (path, line) = path_line.rsplit_once(':')?;
    let line = line.parse::<u64>().ok()?;
    let (severity, rest) = rest.split_once(']')?;
    let mut body = rest.trim();
    let mut source = "";
    if let Some((name, rest)) = body.strip_prefix('[').and_then(|s| s.split_once(']')) {
        source = name;
        body = rest.trim();
    }
    let mut code = "";
    if let Some((message, suffix)) = body.rsplit_once(" (")
        && let Some(identifier) = suffix.strip_suffix(')')
        && !identifier.is_empty()
        && identifier
            .chars()
            .all(|c| c.is_alphanumeric() || "_-./".contains(c))
    {
        code = identifier;
        body = message;
    }
    Some(Diagnostic {
        severity: Severity::parse(&Value::String(severity.into())),
        path: if same_file(path, file) {
            String::new()
        } else {
            path.into()
        },
        line: Some(line),
        column: column.parse().ok(),
        source: source.into(),
        code: code.into(),
        message: body.into(),
        related: Vec::new(),
    })
}
pub fn file_path(uri: &str) -> String {
    let Some(uri) = uri.strip_prefix("file://") else {
        return uri.into();
    };
    let path = uri.strip_prefix("localhost").unwrap_or(uri);
    let mut bytes = Vec::new();
    let mut input = path.as_bytes().iter().copied();
    while let Some(c) = input.next() {
        if c == b'%' {
            let pair = [input.next(), input.next()];
            if let [Some(a), Some(b)] = pair
                && let (Some(a), Some(b)) = ((a as char).to_digit(16), (b as char).to_digit(16))
            {
                bytes.push((a * 16 + b) as u8);
                continue;
            }
            return path.into();
        }
        bytes.push(c);
    }
    let decoded = String::from_utf8(bytes).unwrap_or_else(|_| path.into());
    if decoded.starts_with('/') && decoded.as_bytes().get(2) == Some(&b':') {
        decoded[1..].into()
    } else if !decoded.starts_with('/') {
        format!("//{decoded}")
    } else {
        decoded
    }
}
pub fn markdown(message: &Message) -> String {
    blocks(message)
        .iter()
        .map(|block| {
            fenced(
                &serde_json::to_string(block).expect("diagnostic fields serialize"),
                "omp-lsp",
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn notice(details: Value) -> Message {
        let mut m = Message::new("custom", "<system-notice>raw</system-notice>".into());
        m.label = Some("lsp-late-diagnostic".into());
        m.details = details;
        m
    }
    #[test]
    fn cpp_text_keeps_cross_file_paths_codes_and_template_characters() {
        let m = notice(
            json!({"files":[{"path":"/project/src/a.cpp","summary":"1 error(s), 1 warning(s)","messages":[
                "src/a.cpp:52:35 [error] [clang] No member in std::vector<A<B>> (no_member)",
                "C:\\other\\b.cpp:1:2 [warning] [clangd] Unused (unused-includes)"
            ]}]}),
        );
        let b = blocks(&m);
        let Block::File { summary, .. } = &b[0] else {
            panic!()
        };
        assert_eq!(summary, "1 个错误  1 个警告");
        assert!(!summary.contains('·'));
        let Block::Diagnostic(d) = &b[1] else {
            panic!()
        };
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.location(), "52:35");
        assert!(d.path.is_empty());
        assert_eq!(d.code, "no_member");
        assert!(d.message.contains("std::vector<A<B>>"));
        let Block::Diagnostic(d) = &b[2] else {
            panic!()
        };
        assert_eq!(d.path, r"C:\other\b.cpp");
        assert_eq!(m.text, "<system-notice>raw</system-notice>");
    }
    #[test]
    fn python_standard_lsp_preserves_multiline_messages_and_zero_based_ranges() {
        let m = notice(
            json!({"uri":"file:///project/%E4%B8%AD%E6%96%87%20file.py","diagnostics":[{
                "severity":1,"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":9}},
                "source":"Pyright","code":"reportAssignmentType","message":"Type \"str\" is not assignable to \"int\"\n  Additional explanation\n    Nested detail",
                "relatedInformation":[{"location":{"uri":"file:///project/other.py","range":{"start":{"line":9,"character":2}}},"message":"Declared here"}]
            }]}),
        );
        let b = blocks(&m);
        let Block::File { path, summary } = &b[0] else {
            panic!()
        };
        assert_eq!(path, "/project/中文 file.py");
        assert_eq!(summary, "1 个错误");
        let Block::Diagnostic(d) = &b[1] else {
            panic!()
        };
        assert_eq!(d.location(), "1:5");
        assert_eq!(d.source, "Pyright");
        assert_eq!(d.code, "reportAssignmentType");
        assert!(d.message.contains("\n    Nested detail"));
        assert!(d.related[0].contains("other.py:10:3"));
    }
    #[test]
    fn typescript_rust_and_python_lsp_severities_are_language_independent() {
        for (source, code, severity) in [
            ("typescript", json!(2322), 2),
            ("rust-analyzer", json!("E0308"), 3),
            ("Ruff", json!("F401"), 4),
        ] {
            let m = notice(
                json!({"files":[{"path":"a.file","diagnostics":[{"message":"<T> [] `code`","severity":severity,"source":source,"code":code,"range":{"start":{"line":5,"character":0}}}]}]}),
            );
            let Block::Diagnostic(d) = &blocks(&m)[1] else {
                panic!()
            };
            assert_eq!(d.location(), "6:1");
            assert_eq!(d.source, source);
            assert!(!d.code.is_empty());
            assert_eq!(
                d.severity,
                [Severity::Warning, Severity::Information, Severity::Hint][severity as usize - 2]
            );
            assert_eq!(d.message, "<T> [] `code`");
        }
    }
    #[test]
    fn legacy_notices_and_multiline_python_continuations_are_preserved() {
        let mut m = notice(Value::Null);
        m.text="<system-notice>\nLate LSP diagnostics arrived after the edit returned:\na.py — 1 error(s)\na.py:3:7 [error] [basedpyright] Wrong type (reportArgumentType)\n  Type str cannot be assigned to int\nExtra context\n</system-notice>".into();
        let b = blocks(&m);
        let Block::Diagnostic(d) = &b[1] else {
            panic!()
        };
        assert!(d.message.contains("\n  Type str"));
        assert_eq!(d.line, Some(3));
        assert!(b.last().unwrap().text().contains("Extra context"));
    }
    #[test]
    fn missing_severity_unknown_diagnostics_and_file_uris_do_not_invent_errors() {
        let m = notice(
            json!({"files":[{"path":"file:///C:/folder/a%20b.py","messages":["Unknown output",{"message":"No severity or location"}]}]}),
        );
        let b = blocks(&m);
        assert!(b[0].text().contains("C:/folder/a b.py"));
        for b in &b[1..] {
            let Block::Diagnostic(d) = b else { panic!() };
            assert_eq!(d.severity, Severity::Unknown);
            assert!(d.line.is_none());
        }
        let malformed = notice(json!({"files":[{"messages":["unrecognized file entry"]}]}));
        assert!(
            blocks(&malformed)[0]
                .text()
                .contains("unrecognized file entry")
        );
        assert_eq!(file_path("file:///e%3A/project/a.py"), "e:/project/a.py");
        assert_eq!(file_path("file://server/share/a.py"), "//server/share/a.py");
        assert_eq!(file_path("/project/a\\b.py"), "/project/a\\b.py");
    }
    #[test]
    fn presentation_roundtrips_literal_backticks_and_keeps_unknown_notices_plain() {
        let m = notice(
            json!({"files":[{"path":"long`name.py","messages":["a.py:1:1 [error] [pylsp] Bad ```code``` (E999)"]}]}),
        );
        let source = markdown(&m);
        assert!(source.contains("omp-lsp"));
        assert!(blocks(&m)[1].text().contains("```code```"));
        let mut plain = Message::new(
            "custom",
            "<system-notice>async output</system-notice>".into(),
        );
        plain.label = Some("async-result".into());
        assert_eq!(super::super::display_body(&plain), "async output");
        plain.role = "assistant".into();
        assert_eq!(super::super::display_body(&plain), plain.text);
    }
}

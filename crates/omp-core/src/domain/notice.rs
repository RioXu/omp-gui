//! Display-only notice formatting; original messages and journals are preserved.
use super::Message;
use std::borrow::Cow;
pub mod lsp;
pub fn display_body(message: &Message) -> Cow<'_, str> {
    if lsp::is_lsp(message) {
        return Cow::Owned(lsp::markdown(message));
    }
    if message.role == "custom" {
        Cow::Borrowed(unwrap_notice(&message.text))
    } else {
        Cow::Borrowed(&message.text)
    }
}
fn unwrap_notice(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix("<system-notice>")
        .and_then(|s| s.strip_suffix("</system-notice>"))
        .map(str::trim)
        .unwrap_or(text)
}

fn summary_text(summary: &str) -> String {
    summary
        .split(',')
        .map(|part| {
            let part = part.trim();
            let Some((count, kind)) = part.split_once(' ') else {
                return part.to_owned();
            };
            if count.parse::<u64>().is_err() {
                return part.to_owned();
            }
            match kind {
                "error(s)" | "errors" | "error" => format!("{count} 个错误"),
                "warning(s)" | "warnings" | "warning" => format!("{count} 个警告"),
                "hint(s)" | "hints" | "hint" => format!("{count} 个提示"),
                _ => part.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

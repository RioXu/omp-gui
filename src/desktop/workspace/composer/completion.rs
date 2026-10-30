use super::*;
use gpui_kit::component::input::InlineToken;

pub(crate) struct Completion {
    pub label: String,
    pub description: String,
    pub replacement: String,
    pub range: std::ops::Range<usize>,
    pub file: bool,
}

// Byte offsets match TextareaState; delimiters are checked on character boundaries.
fn trigger(text: &str, cursor: usize) -> Option<(bool, std::ops::Range<usize>, &str)> {
    let before = text.get(..cursor)?;
    if let Some(query) = before.strip_prefix('/')
        && !query.chars().any(char::is_whitespace)
    {
        return Some((false, 0..cursor, query));
    }
    for (ix, ch) in before.char_indices().rev() {
        if ch == '@'
            && (ix == 0
                || before[..ix].ends_with(|c: char| c.is_whitespace() || "([{<\"'`".contains(c)))
        {
            let query = &before[ix + 1..];
            if let Some(quoted) = query.strip_prefix('"') {
                if !quoted.contains(['"', '\n']) {
                    return Some((true, ix..cursor, quoted));
                }
            } else if !query.chars().any(char::is_whitespace) {
                return Some((true, ix..cursor, query));
            }
        }
    }
    None
}

fn replacement_range(text: &str, mut range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let suffix = &text[range.end..];
    let quoted = text[range.start..].starts_with("@\"");
    let end = if quoted {
        suffix.find('"').map(|ix| ix + 1).unwrap_or(0)
    } else {
        suffix.find(char::is_whitespace).unwrap_or(suffix.len())
    };
    range.end += end;
    range
}

impl Workspace {
    pub fn move_completion(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let count = self.completions(cx).len();
        if count > 0 {
            self.composer_state.selected_completion =
                (self.composer_state.selected_completion.min(count - 1)
                    + if backwards { count - 1 } else { 1 })
                    % count;
            cx.notify();
        }
    }
    pub fn completion_hint(&self, cx: &App) -> Option<&'static str> {
        if self.composer_state.dismissed {
            return None;
        }
        let input = self.composer_state.input.read(cx);
        let text = input.value();
        let (file, range, _) = trigger(&text, input.cursor())?;
        if input
            .tokens()
            .iter()
            .any(|token| token.range().contains(&range.start))
        {
            return None;
        }
        Some(if file {
            if self.document_state.loading {
                "正在读取项目文件…"
            } else {
                "未找到匹配文件"
            }
        } else if !self.connected {
            "连接 OMP 后显示可用命令"
        } else if self.composer_state.commands.is_empty() {
            "正在读取 OMP 命令…"
        } else {
            "未找到匹配命令"
        })
    }

    pub fn completions(&self, cx: &App) -> Vec<Completion> {
        if self.composer_state.dismissed {
            return vec![];
        }
        let input = self.composer_state.input.read(cx);
        let text = input.value();
        let Some((file, range, query)) = trigger(&text, input.cursor()) else {
            return vec![];
        };
        if input
            .tokens()
            .iter()
            .any(|token| token.range().contains(&range.start))
        {
            return vec![];
        }
        let query = query.to_lowercase();
        let range = replacement_range(&text, range);
        if file {
            self.document_state
                .files
                .iter()
                .filter_map(|p| {
                    let path = p.to_string_lossy().replace('\\', "/");
                    path.to_lowercase().contains(&query).then(|| Completion {
                        label: path.clone(),
                        description: "引用项目文件".into(),
                        replacement: format!("@\"{path}\""),
                        range: range.clone(),
                        file: true,
                    })
                })
                .take(50)
                .collect()
        } else {
            let mut commands = self.composer_state.commands.clone();
            for (name, description) in [
                ("model", "选择和配置模型"),
                ("models", "选择和配置模型"),
                ("switch", "选择模型"),
                ("effort", "配置思考等级"),
                ("mcp", "配置 MCP"),
                ("plugins", "管理插件"),
                ("settings", "打开设置"),
                ("lsp", "配置语言服务器"),
                ("skills", "管理技能"),
                ("thinking", "配置思考等级"),
            ] {
                if !commands.iter().any(|c| c["name"] == name) {
                    commands.push(json!({"name":name,"description":description,"source":"gui"}));
                }
            }
            commands
                .iter()
                .filter_map(|c| {
                    let name = c["name"].as_str()?.trim_start_matches('/');
                    let matches = name.to_lowercase().contains(&query)
                        || c["aliases"].as_array().is_some_and(|a| {
                            a.iter().any(|a| {
                                a.as_str()
                                    .is_some_and(|a| a.to_lowercase().contains(&query))
                            })
                        });
                    matches.then(|| Completion {
                        label: format!("/{name}"),
                        description: if let Some((category, _)) =
                            super::settings_commands::settings_target(&format!("/{name}"))
                        {
                            format!("打开设置 · {category}")
                        } else {
                            c["description"].as_str().unwrap_or("").into()
                        },
                        replacement: format!("/{name} "),
                        range: range.clone(),
                        file: false,
                    })
                })
                .take(50)
                .collect()
        }
    }

    pub fn accept_completion(&mut self, ix: usize, w: &mut Window, cx: &mut Context<Self>) {
        let items = self.completions(cx);
        let ix = ix.min(items.len().saturating_sub(1));
        let Some(item) = items.into_iter().nth(ix) else {
            return;
        };
        if !item.file {
            let mut completed = self.composer_state.input.read(cx).value().to_string();
            completed.replace_range(item.range.clone(), &item.replacement);
            if self.route_settings_command(&completed, w, cx) {
                return;
            }
        }
        self.composer_state.input.update(cx, |input, cx| {
            if item.file {
                let token = InlineToken::new(item.label.clone(), item.replacement)
                    .with_label(format!("@{}", item.label));
                let _ = input.replace_range_with_token(item.range, token, w, cx);
                input.insert(" ", w, cx);
            } else {
                input.set_selected_range(item.range, cx);
                input.replace(item.replacement, w, cx);
            }
        });
        // Programmatic edits also save drafts, including the text behind tokens.
        let draft = self.composer_state.input.read(cx).value().to_string();
        if let Some(s) = self.session_mut() {
            s.draft = draft;
            self.dirty = true;
        }
        self.composer_state.dismissed = true;
        self.composer_state.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{replacement_range, trigger};
    #[test]
    fn completion_replaces_the_whole_word_at_a_unicode_cursor() {
        let text = "查看 @src/文件.rs 后续";
        let (_, range, _) = trigger(text, "查看 @src/文".len()).unwrap();
        assert_eq!(&text[replacement_range(text, range)], "@src/文件.rs");
        assert_eq!(replacement_range("/model arg", 0..3), 0..6);
        assert_eq!(
            &"@\"my file.txt\" tail"[replacement_range("@\"my file.txt\" tail", 0..5)],
            "@\"my file.txt\""
        );
    }
    #[test]
    fn triggers_respect_unicode_cursor_and_command_position() {
        assert_eq!(
            trigger("看 @src/文件.rs 后文", "看 @src/文".len()),
            Some((true, "看 ".len().."看 @src/文".len(), "src/文"))
        );
        assert_eq!(trigger("/mod", 4), Some((false, 0..4, "mod")));
        assert!(trigger("mail@host", 9).is_none());
        assert!(trigger("文字 /mod", "文字 /mod".len()).is_none());
        assert!(trigger("/model arg", 10).is_none());
        assert!(trigger("@src/main.rs 后文", "@src/main.rs 后文".len()).is_none());
        assert_eq!(trigger("@\"my file", 9), Some((true, 0..9, "my file")));
        assert!(trigger("@\"my file\" done", 15).is_none());
        assert!(trigger("中文", 1).is_none());
    }
}

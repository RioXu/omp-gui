use super::*;

pub(crate) fn settings_target(text: &str) -> Option<(&'static str, &'static str)> {
    let mut words = text.split_whitespace();
    let command = words.next()?;
    let argument = words.next();
    if words.next().is_some() {
        return None;
    }
    if command == "/advisor" && argument == Some("configure") {
        return Some(("角色与提示词", "advisor."));
    }
    if argument.is_some() {
        return None;
    }
    Some(match command {
        "/model" | "/models" | "/switch" => ("模型", ""),
        "/effort" | "/thinking" => ("模型", "defaultThinkingLevel"),
        "/modelpreset" => ("角色与提示词", "modelPreset"),
        "/settings" => ("常规", ""),
        "/mcp" => ("MCP", ""),
        "/lsp" => ("LSP", ""),
        "/skills" => ("技能", ""),
        "/plugins" | "/plugin" => ("插件", ""),
        _ => return None,
    })
}

impl Workspace {
    pub(super) fn route_settings_command(
        &mut self,
        text: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((category, query)) = settings_target(text) else {
            return false;
        };
        self.settings(w, cx);
        if let Some(view) = &self.settings_view {
            view.update(cx, |view, cx| view.navigate(category, query, w, cx));
        }
        self.input
            .update(cx, |input, cx| input.set_value("", w, cx));
        if let Some(s) = self.session_mut() {
            s.draft.clear();
        }
        self.completion_dismissed = true;
        self.dirty = true;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::settings_target;
    #[test]
    fn interactive_configuration_routes_without_swallowing_rpc_arguments() {
        assert_eq!(settings_target(" /model \n"), Some(("模型", "")));
        assert_eq!(settings_target("/models"), Some(("模型", "")));
        assert_eq!(
            settings_target("/effort"),
            Some(("模型", "defaultThinkingLevel"))
        );
        assert_eq!(settings_target("/mcp"), Some(("MCP", "")));
        assert_eq!(
            settings_target("/advisor configure"),
            Some(("角色与提示词", "advisor."))
        );
        for text in [
            "/effort high",
            "/switch deepseek",
            "/mcp list",
            "/plugins enable a",
            "/rename name",
            "/modelpreset save name",
            "explain /model",
        ] {
            assert_eq!(settings_target(text), None, "{text}");
        }
    }
}

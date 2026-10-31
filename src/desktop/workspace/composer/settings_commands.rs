use super::*;
use crate::desktop::settings::Destination;

pub(crate) fn settings_target(text: &str) -> Option<Destination> {
    let mut words = text.split_whitespace();
    let command = words.next()?;
    let argument = words.next();
    if words.next().is_some() {
        return None;
    }
    if command == "/advisor" && argument == Some("configure") {
        return Some(Destination::Page("角色与提示词", "advisor."));
    }
    if argument.is_some() {
        return None;
    }
    Some(match command {
        "/model" | "/models" | "/switch" => Destination::ModelServices,
        "/fast" | "/slow" => Destination::ProcessingSpeed,
        "/tools" => Destination::Page("工具调用", ""),
        "/effort" | "/thinking" => Destination::Page("模型", "defaultThinkingLevel"),
        "/modelpreset" => Destination::Page("角色与提示词", "modelPreset"),
        "/settings" => Destination::Page("常规", ""),
        "/mcp" => Destination::Page("MCP", ""),
        "/lsp" => Destination::Page("LSP", ""),
        "/skills" => Destination::Page("技能", ""),
        "/plugins" | "/plugin" => Destination::Page("插件", ""),
        _ => return None,
    })
}

impl Workspace {
    pub(in crate::desktop::workspace) fn route_settings_command(
        &mut self,
        text: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(destination) = settings_target(text) else {
            return false;
        };
        self.settings(w, cx);
        if let Some(view) = &self.settings_view {
            view.update(cx, |view, cx| view.navigate(destination, w, cx));
        }
        self.composer_state
            .input
            .update(cx, |input, cx| input.set_value("", w, cx));
        if let Some(s) = self.session_mut() {
            s.draft.clear();
        }
        self.composer_state.dismissed = true;
        self.dirty = true;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Destination, settings_target};
    #[test]
    fn interactive_configuration_routes_without_swallowing_rpc_arguments() {
        assert_eq!(
            settings_target(" /model \n"),
            Some(Destination::ModelServices)
        );
        assert_eq!(settings_target("/models"), Some(Destination::ModelServices));
        for cmd in ["/fast", "/slow"] {
            assert_eq!(settings_target(cmd), Some(Destination::ProcessingSpeed));
        }
        assert_eq!(
            settings_target("/effort"),
            Some(Destination::Page("模型", "defaultThinkingLevel"))
        );
        assert_eq!(settings_target("/mcp"), Some(Destination::Page("MCP", "")));
        assert_eq!(
            settings_target("/advisor configure"),
            Some(Destination::Page("角色与提示词", "advisor."))
        );
        for text in [
            "/effort high",
            "/fast on",
            "/slow off",
            "/model deepseek/model",
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

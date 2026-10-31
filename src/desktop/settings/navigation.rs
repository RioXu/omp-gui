//! Concrete GUI destinations for commands that normally open TUI configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Page(&'static str, &'static str),
    ModelServices,
    ProcessingSpeed,
}

impl Destination {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Page(category, _) => category,
            Self::ModelServices => "模型服务与注册",
            Self::ProcessingSpeed => "速度与服务等级",
        }
    }
}

pub(super) fn speed_setting(key: &str) -> bool {
    key.starts_with("tier.") || key == "providers.anthropic.slowMode"
}

#[cfg(test)]
mod tests {
    #[test]
    fn speed_destination_includes_anthropic_slow_mode_without_other_provider_fields() {
        for key in [
            "tier.openai",
            "tier.google",
            "tier.anthropic",
            "tier.subagent",
            "providers.anthropic.slowMode",
        ] {
            assert!(super::speed_setting(key));
        }
        for key in [
            "modelRoles",
            "providers.anthropic.apiKey",
            "defaultThinkingLevel",
        ] {
            assert!(!super::speed_setting(key));
        }
    }
}

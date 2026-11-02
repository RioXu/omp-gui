use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Window, rgb};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skin {
    Light,
    Dark,
    Obsidian,
}
impl Skin {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::Obsidian];
    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Obsidian => "obsidian-orange",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "默认白",
            Self::Dark => "默认黑",
            Self::Obsidian => "曜石橙",
        }
    }
    pub fn from_saved(value: &str, dark: bool) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "obsidian-orange" => Self::Obsidian,
            _ if dark => Self::Dark,
            _ => Self::Light,
        }
    }
}

pub(crate) fn apply(skin: Skin, window: Option<&mut Window>, cx: &mut App) {
    // Reload the native palette even for dark-to-dark changes, avoiding stale
    // orange tokens when returning to the default black skin.
    Theme::change(
        if skin == Skin::Light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        window,
        cx,
    );
    apply_accent(cx);
    let installed = cx.text_system().all_font_names();
    let sans = if skin == Skin::Obsidian && installed.iter().any(|name| name == "Noto Sans SC") {
        "Noto Sans SC"
    } else {
        "Microsoft YaHei UI"
    };
    let mono = if skin == Skin::Obsidian && installed.iter().any(|name| name == "Cascadia Mono") {
        "Cascadia Mono".into()
    } else {
        gpui_kit::base::TypographyTokens::default().mono
    };
    Theme::update(cx, |t| {
        t.font_family = sans.into();
        t.mono_font_family = mono;
        if skin != Skin::Obsidian {
            return;
        }
        t.background = rgb(0x090b0c).into();
        t.foreground = rgb(0xf4f6f7).into();
        t.sidebar = rgb(0x111517).into();
        t.sidebar_foreground = t.foreground;
        t.muted = rgb(0x1c2225).into();
        t.muted_foreground = rgb(0xaab4ba).into();
        t.border = rgb(0x394348).into();
        t.input = t.border;
        t.sidebar_border = t.border;
        t.primary = rgb(0xff6600).into();
        t.primary_hover = rgb(0xff8533).into();
        t.primary_active = rgb(0xe65000).into();
        t.primary_foreground = rgb(0x0b0d0e).into();
        t.button_primary = t.primary;
        t.button_primary_hover = t.primary_hover;
        t.button_primary_active = t.primary_active;
        t.button_primary_foreground = t.primary_foreground;
        t.ring = t.primary;
        t.caret = t.primary;
        t.link = t.primary;
        t.link_hover = t.primary_hover;
        t.link_active = t.primary_active;
        t.accent = rgb(0x39200f).into();
        t.accent_foreground = rgb(0xff9a57).into();
        t.sidebar_accent = t.accent;
        t.sidebar_accent_foreground = t.accent_foreground;
        t.sidebar_primary = t.primary;
        t.sidebar_primary_foreground = t.primary_foreground;
        t.selection = t.primary.opacity(0.28);
        t.popover = rgb(0x171c1f).into();
        t.popover_foreground = t.foreground;
        t.button = t.muted;
        t.button_foreground = t.foreground;
        t.button_hover = rgb(0x293136).into();
        t.button_active = t.accent;
        t.secondary = t.muted;
        t.secondary_foreground = t.foreground;
        t.secondary_hover = t.button_hover;
        t.secondary_active = t.accent;
        t.button_secondary = t.secondary;
        t.button_secondary_foreground = t.foreground;
        t.button_secondary_hover = t.secondary_hover;
        t.button_secondary_active = t.accent;
        t.colors.list = t.background;
        t.list_hover = t.muted;
        t.list_active = t.accent;
        t.list_active_border = t.primary;
        t.tab = t.sidebar;
        t.tab_bar = t.sidebar;
        t.tab_foreground = t.muted_foreground;
        t.tab_active = t.background;
        t.tab_active_foreground = t.foreground;
        t.title_bar = t.sidebar;
        t.title_bar_border = t.border;
        t.scrollbar_thumb = rgb(0x536168).into();
        t.scrollbar_thumb_hover = rgb(0x788991).into();
        let background = t.background;
        let syntax = std::sync::Arc::make_mut(&mut t.highlight_theme);
        syntax.style.editor_background = Some(background);
        syntax.style.editor_gutter_background = None;
    });
    cx.refresh_windows();
}

pub(crate) fn apply_accent(cx: &mut App) {
    Theme::update(cx, |t| {
        // Short, critically damped movement keeps tab underlines responsive.
        t.motion.spring_move = gpui_kit::base::Spring::new(std::time::Duration::from_millis(140))
            .with_damping(1.0)
            .with_epsilon(0.25);
        t.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Hover;
        let dark = t.is_dark();
        t.background = rgb(if dark { 0x101216 } else { 0xffffff }).into();
        // Native Editor paints the gutter independently, even without its frame.
        // Use the document surface and let the gutter inherit that same color.
        let editor_background = t.background;
        let syntax = std::sync::Arc::make_mut(&mut t.highlight_theme);
        syntax.style.editor_background = Some(editor_background);
        syntax.style.editor_gutter_background = None;
        t.foreground = rgb(if dark { 0xe6e9ef } else { 0x202631 }).into();
        t.sidebar = rgb(if dark { 0x14171c } else { 0xf7f8fa }).into();
        t.muted = rgb(if dark { 0x1b1f26 } else { 0xf2f4f7 }).into();
        t.muted_foreground = rgb(if dark { 0xa5afbe } else { 0x596575 }).into();
        t.border = rgb(if dark { 0x303743 } else { 0xdde2e9 }).into();
        let blue = gpui_kit::rgb(if dark { 0x8baaff } else { 0x245bdb }).into();
        t.primary = blue;
        t.primary_hover = blue;
        t.primary_active = blue;
        t.primary_foreground = gpui_kit::rgb(if dark { 0x101827 } else { 0xffffff }).into();
        t.button_primary = blue;
        t.button_primary_hover = blue;
        t.button_primary_active = blue;
        t.button_primary_foreground = t.primary_foreground;
        t.sidebar_accent = gpui_kit::rgb(if t.is_dark() { 0x213454 } else { 0xdbeafe }).into();
        t.sidebar_accent_foreground =
            gpui_kit::rgb(if t.is_dark() { 0xbcd3ff } else { 0x1d4ed8 }).into();
        t.accent = t.sidebar_accent;
        t.accent_foreground = t.sidebar_accent_foreground;
        t.list_active = t.sidebar_accent;
        t.list_active_border = t.primary;
        t.secondary_active = t.sidebar_accent;
        t.button_active = t.sidebar_accent;
        t.tab_active = t.background;
        t.tab_active_foreground = t.foreground;
    });
}

/// A distinct, readable secondary voice for reasoning in every skin.
pub(crate) fn thinking_color(cx: &App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    rgb(if cx.theme().is_dark() {
        0xa4b7cb
    } else {
        0x506780
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::{Skin, apply};
    use gpui_kit::{TestAppContext, component::ActiveTheme};

    #[gpui_kit::test]
    fn skins_restore_palette_fonts_and_native_tokens_when_switching(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            apply(Skin::Dark, None, cx);
            let original = cx.theme().clone();
            apply(Skin::Obsidian, None, cx);
            let orange = cx.theme();
            assert_ne!(orange.primary, original.primary);
            assert_eq!(orange.tokens.background.color, orange.background);
            assert_eq!(
                orange.highlight_theme.style.editor_background,
                Some(orange.background)
            );
            apply(Skin::Dark, None, cx);
            assert_eq!(cx.theme().colors, original.colors);
            assert_eq!(cx.theme().font_family, original.font_family);
            assert_eq!(cx.theme().mono_font_family, original.mono_font_family);
            apply(Skin::Light, None, cx);
            assert!(!cx.theme().is_dark());
            assert_eq!(
                Skin::from_saved("obsidian-orange", false).id(),
                "obsidian-orange"
            );
            assert_eq!(Skin::from_saved("system", true).id(), "dark");
        });
    }
}

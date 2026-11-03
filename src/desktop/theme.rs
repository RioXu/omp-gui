use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Window, rgb};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skin {
    Light,
    Dark,
    Obsidian,
    Paper,
    Rose,
    Peach,
    Butter,
    Latte,
}
impl Skin {
    pub const ALL: [Self; 8] = [
        Self::Light,
        Self::Dark,
        Self::Rose,
        Self::Peach,
        Self::Butter,
        Self::Latte,
        Self::Paper,
        Self::Obsidian,
    ];
    pub fn is_light(self) -> bool {
        !matches!(self, Self::Dark | Self::Obsidian)
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Obsidian => "obsidian-orange",
            Self::Paper => "paper",
            Self::Rose => "macaron-rose",
            Self::Peach => "macaron-peach",
            Self::Butter => "macaron-butter",
            Self::Latte => "macaron-latte",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "瓷白 · 极简",
            Self::Dark => "石墨 · 极简",
            Self::Obsidian => "曜石橙",
            Self::Paper => "暖纸白",
            Self::Rose => "草莓奶霜",
            Self::Peach => "蜜桃软糖",
            Self::Butter => "香草布丁",
            Self::Latte => "燕麦拿铁",
        }
    }
    pub fn from_saved(value: &str, dark: bool) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "obsidian-orange" => Self::Obsidian,
            "paper" => Self::Paper,
            "macaron-rose" => Self::Rose,
            "macaron-peach" => Self::Peach,
            "macaron-butter" => Self::Butter,
            "macaron-latte" => Self::Latte,
            // Retired cool dark skins migrate to the neutral dark palette.
            "forest" | "dusk" => Self::Dark,
            _ if dark => Self::Dark,
            _ => Self::Light,
        }
    }
}

pub(crate) fn apply(skin: Skin, window: Option<&mut Window>, cx: &mut App) {
    // Reload the native palette even for dark-to-dark changes, avoiding stale
    // orange tokens when returning to the default black skin.
    Theme::change(
        if skin.is_light() {
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
        t.radius = gpui_kit::px(8.);
        t.radius_lg = gpui_kit::px(14.);
        // Every skin defines the same semantic surfaces. Native menus, editors,
        // tabs and buttons therefore switch together, including dark-to-dark.
        let (
            background,
            foreground,
            sidebar,
            surface,
            muted,
            border,
            primary,
            hover,
            pressed,
            accent,
            accent_text,
        ) = match skin {
            Skin::Light => (
                0xf5f5f7, 0x1d1d1f, 0xebebed, 0xffffff, 0x656568, 0xd8d8dc, 0x48484b, 0x353538,
                0x242426, 0xe3e3e7, 0x343437,
            ),
            Skin::Dark => (
                0x1c1c1e, 0xf5f5f7, 0x232325, 0x2c2c2e, 0xb3b3b8, 0x444447, 0xd5d5d9, 0xeeeef0,
                0xbcbcc2, 0x3a3a3e, 0xe5e5e9,
            ),
            Skin::Obsidian => (
                0x101112, 0xf4f3f0, 0x171819, 0x222324, 0xb9b6b1, 0x41403e, 0xff995c, 0xffb483,
                0xec8042, 0x3b291e, 0xffbb90,
            ),
            Skin::Paper => (
                0xfaf7f0, 0x39352f, 0xf0ebe1, 0xfffdf8, 0x6e6559, 0xdcd3c3, 0x895a36, 0x7d5030,
                0x684128, 0xeee1cf, 0x78502d,
            ),
            Skin::Rose => (
                0xfff8f8, 0x46383c, 0xf8e9eb, 0xfffdfc, 0x7b6268, 0xe6cdd1, 0x965565, 0x824655,
                0x703c49, 0xf4dce1, 0x754351,
            ),
            Skin::Peach => (
                0xfff8f3, 0x493a33, 0xfae9df, 0xfffdf9, 0x7d6559, 0xe8cfc0, 0x985b42, 0x824b35,
                0x6f3e2c, 0xf8dfcd, 0x7d4b34,
            ),
            Skin::Butter => (
                0xfffbef, 0x443d2e, 0xf8f0d8, 0xfffef8, 0x75694d, 0xe4d8b6, 0x856724, 0x71561d,
                0x604919, 0xf3e6bb, 0x705821,
            ),
            Skin::Latte => (
                0xfaf6f2, 0x423932, 0xeee5dc, 0xfffcf8, 0x706154, 0xded0c3, 0x795740, 0x704f3b,
                0x5f4232, 0xead9c9, 0x6d503e,
            ),
        };
        t.background = rgb(background).into();
        t.foreground = rgb(foreground).into();
        t.sidebar = rgb(sidebar).into();
        t.sidebar_foreground = t.foreground;
        t.muted = rgb(surface).into();
        t.muted_foreground = rgb(muted).into();
        t.border = rgb(border).into();
        t.input = t.border;
        t.sidebar_border = t.border;
        t.primary = rgb(primary).into();
        t.primary_hover = rgb(hover).into();
        t.primary_active = rgb(pressed).into();
        t.primary_foreground = rgb(if skin.is_light() {
            0xffffff
        } else {
            background
        })
        .into();
        t.button_primary = t.primary;
        t.button_primary_hover = t.primary_hover;
        t.button_primary_active = t.primary_active;
        t.button_primary_foreground = t.primary_foreground;
        // Pastel fills with dark lettering keep the macaron buttons soft and
        // readable. Links and focus rings retain the deeper semantic accent.
        if let Some((fill, hover, pressed)) = match skin {
            Skin::Rose => Some((0xefc7d0, 0xe9b8c4, 0xe0a9b7)),
            Skin::Peach => Some((0xf4cfb7, 0xedbfa2, 0xe4ae8e)),
            Skin::Butter => Some((0xefdda0, 0xe6d18a, 0xdbc276)),
            Skin::Latte => Some((0xdec8b5, 0xd2b9a3, 0xc6a98f)),
            _ => None,
        } {
            t.button_primary = rgb(fill).into();
            t.button_primary_hover = rgb(hover).into();
            t.button_primary_active = rgb(pressed).into();
            t.button_primary_foreground = rgb(foreground).into();
        }
        t.ring = t.primary;
        t.caret = t.primary;
        t.link = t.primary;
        t.link_hover = t.primary_hover;
        t.link_active = t.primary_active;
        t.accent = rgb(accent).into();
        t.accent_foreground = rgb(accent_text).into();
        t.sidebar_accent = t.accent;
        t.sidebar_accent_foreground = t.accent_foreground;
        t.sidebar_primary = t.primary;
        t.sidebar_primary_foreground = t.primary_foreground;
        t.selection = t.primary.opacity(0.28);
        t.popover = t.muted;
        t.popover_foreground = t.foreground;
        t.button = t.muted;
        t.button_foreground = t.foreground;
        t.button_hover = t.accent;
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
        t.scrollbar_thumb = t.muted_foreground.opacity(0.35);
        t.scrollbar_thumb_hover = t.muted_foreground.opacity(0.65);
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
        t.motion.spring_move = gpui_kit::base::Spring::new(std::time::Duration::from_millis(90))
            .with_damping(1.0)
            .with_epsilon(0.25);
        t.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Hover;
        let dark = t.is_dark();
        t.background = rgb(if dark { 0x1c1c1e } else { 0xf5f5f7 }).into();
        // Native Editor paints the gutter independently, even without its frame.
        // Use the document surface and let the gutter inherit that same color.
        let editor_background = t.background;
        let syntax = std::sync::Arc::make_mut(&mut t.highlight_theme);
        syntax.style.editor_background = Some(editor_background);
        syntax.style.editor_gutter_background = None;
        t.foreground = rgb(if dark { 0xf5f5f7 } else { 0x1d1d1f }).into();
        t.sidebar = rgb(if dark { 0x232325 } else { 0xebebed }).into();
        t.muted = rgb(if dark { 0x2c2c2e } else { 0xffffff }).into();
        t.muted_foreground = rgb(if dark { 0xb3b3b8 } else { 0x656568 }).into();
        t.border = rgb(if dark { 0x444447 } else { 0xd8d8dc }).into();
        t.primary = rgb(if dark { 0xd5d5d9 } else { 0x48484b }).into();
        t.primary_hover = rgb(if dark { 0xeeeef0 } else { 0x353538 }).into();
        t.primary_active = rgb(if dark { 0xbcbcc2 } else { 0x242426 }).into();
        t.primary_foreground = gpui_kit::rgb(if dark { 0x1c1c1e } else { 0xffffff }).into();
        t.button_primary = t.primary;
        t.button_primary_hover = t.primary_hover;
        t.button_primary_active = t.primary_active;
        t.button_primary_foreground = t.primary_foreground;
        t.sidebar_accent = gpui_kit::rgb(if dark { 0x3a3a3e } else { 0xe3e3e7 }).into();
        t.sidebar_accent_foreground = gpui_kit::rgb(if dark { 0xe5e5e9 } else { 0x343437 }).into();
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
    cx.theme().muted_foreground
}

/// A quiet tinted sheen, painted by GPUI without a blur pass or animation loop.
pub(crate) fn surface_sheen(cx: &App) -> gpui_kit::Background {
    use gpui_kit::component::ActiveTheme;
    gpui_kit::linear_gradient(
        135.,
        gpui_kit::linear_color_stop(cx.theme().muted, 0.),
        gpui_kit::linear_color_stop(cx.theme().background, 1.),
    )
}

#[cfg(test)]
mod tests {
    use super::{Skin, apply};
    use gpui_kit::{TestAppContext, component::ActiveTheme};

    fn contrast(a: gpui_kit::Hsla, b: gpui_kit::Hsla) -> f32 {
        let luminance = |color: gpui_kit::Hsla| {
            let rgb = gpui_kit::Rgba::from(color);
            let linear = |c: f32| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[gpui_kit::test]
    fn skins_restore_palette_fonts_and_native_tokens_when_switching(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            apply(Skin::Dark, None, cx);
            let original = cx.theme().clone();
            for skin in Skin::ALL {
                assert_eq!(
                    Skin::from_saved(skin.id(), !skin.is_light()).id(),
                    skin.id()
                );
                apply(skin, None, cx);
                let theme = cx.theme();
                assert_eq!(theme.is_dark(), !skin.is_light());
                assert_eq!(theme.tokens.background.color, theme.background);
                assert_eq!(
                    theme.highlight_theme.style.editor_background,
                    Some(theme.background)
                );
                // Exercise every skin's actual composed surfaces, including
                // pastel buttons and their hover/pressed states.
                for surface in [theme.background, theme.sidebar, theme.muted] {
                    for text in [theme.foreground, theme.muted_foreground, theme.link] {
                        assert!(
                            contrast(surface, text) >= 4.5,
                            "{} text contrast",
                            skin.id()
                        );
                    }
                }
                for fill in [
                    theme.button_primary,
                    theme.button_primary_hover,
                    theme.button_primary_active,
                ] {
                    assert!(
                        contrast(fill, theme.button_primary_foreground) >= 4.5,
                        "{} button contrast",
                        skin.id()
                    );
                }
                assert!(contrast(theme.accent, theme.accent_foreground) >= 4.5);
                assert_ne!(theme.primary, theme.primary_hover);
                apply(Skin::Dark, None, cx);
                assert_eq!(cx.theme().colors, original.colors);
            }
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
            assert_eq!(Skin::from_saved("forest", false).id(), "dark");
            assert_eq!(Skin::from_saved("dusk", false).id(), "dark");
        });
    }
}

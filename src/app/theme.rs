use super::*;
pub(crate) fn apply_accent(cx: &mut App) {
    Theme::update(cx, |t| {
        let dark = t.is_dark();
        t.background = rgb(if dark { 0x101216 } else { 0xffffff }).into();
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
        t.tab_active = t.sidebar_accent;
        t.tab_active_foreground = t.sidebar_accent_foreground;
    });
}

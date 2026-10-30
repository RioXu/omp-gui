use super::*;
pub(crate) fn apply_accent(cx: &mut App) {
    Theme::update(cx, |t| {
        let blue = gpui_kit::rgb(if t.is_dark() { 0x608fff } else { 0x2563eb }).into();
        t.primary = blue;
        t.primary_hover = blue;
        t.primary_active = blue;
        t.primary_foreground = gpui_kit::rgb(0xffffff).into();
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

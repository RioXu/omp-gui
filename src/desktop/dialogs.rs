//! Shared presentation for workspace dialogs, using the native motion lifecycle.
use gpui_kit::{
    App, Window,
    component::{
        WindowExt,
        dialog::{Dialog, DialogEntrance},
    },
};

pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
) {
    // FadeSlide moves only the theme's short distance after layout is resolved.
    // Native presence handles interrupted exits, focus and reduced motion.
    window.open_dialog(cx, move |dialog, window, cx| {
        build(dialog.entrance(DialogEntrance::FadeSlide), window, cx)
    });
}

#[cfg(test)]
mod tests;

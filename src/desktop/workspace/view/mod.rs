mod activity;
mod attachments;
mod comment_editor;
mod composer;
mod composer_tasks;
mod conversation;
#[cfg(test)]
mod conversation_layout_tests;
mod documents;
mod notice;
mod panel;
mod progress;
mod requests;
mod search;
mod selection;
mod shell;
pub(in crate::desktop::workspace) mod sidebar;
mod subagents;
mod thinking;
mod transcript;
mod working_intent;

use super::*;
use chrono::{Local, TimeZone};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    diff::Diff,
    input::{Input, Textarea},
    list::ListItem,
    menu::{DropdownMenu, PopupMenuItem},
    popover::Popover,
    resizable::resize_handle,
    sidebar::{SidebarItem, SidebarMenu, SidebarMenuItem},
    tab::{Tab, TabBar},
    text::TextView,
    toolbar::Toolbar,
    tree::Tree,
};
use gpui_kit::{prelude::FluentBuilder, *};

const PANE_HEADER_HEIGHT: f32 = 36.;

fn icon_spinner() -> gpui_kit::component::spinner::Spinner {
    gpui_kit::component::spinner::Spinner::new().icon(assets::IconName::LoaderCircle)
}
#[derive(Clone)]
struct ResizeSide(bool);

mod changed_files;
use changed_files::change_color;

mod comment_editor;
mod composer;
mod conversation;
mod documents;
mod panel;
mod progress;
mod requests;
mod search;
mod selection;
mod shell;
mod sidebar;
mod transcript;

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
    text::TextView,
    tree::Tree,
};
use gpui_kit::{prelude::FluentBuilder, *};
#[derive(Clone)]
struct ResizeSide(bool);

mod changed_files;
use changed_files::change_color;

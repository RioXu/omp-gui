mod comment_editor;
mod composer;
mod conversation;
mod panel;
mod progress;
mod requests;
mod sidebar;
mod transcript;
mod view;

use crate::{app::*, domain::*};
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
use serde_json::json;
#[derive(Clone)]
struct ResizeSide(bool);

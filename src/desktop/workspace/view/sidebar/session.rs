use super::*;

#[derive(Clone)]
pub(super) struct SessionNavigation {
    pub id: SharedString,
    pub title: SharedString,
    pub archived: bool,
    pub active: bool,
    pub running: bool,
    pub disabled: bool,
    pub workspace: WeakEntity<Workspace>,
}
#[derive(Clone)]
pub(super) enum NavigationItem {
    Project(ProjectNavigation),
    Session(SessionNavigation),
    Empty(bool),
}
impl gpui_kit::component::Collapsible for NavigationItem {
    fn is_collapsed(&self) -> bool {
        false
    }
    fn collapsed(self, _: bool) -> Self {
        self
    }
}
impl SidebarItem for NavigationItem {
    fn render(
        self,
        _: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        match self {
            Self::Project(project) => project.render("project", window, cx).into_any_element(),
            Self::Empty(archived) => div()
                .ml_3p5()
                .pl_2p5()
                .border_l_1()
                .border_color(cx.theme().sidebar_border)
                .py_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if archived {
                    "暂无已归档会话"
                } else {
                    "点击 + 开始会话"
                })
                .into_any_element(),
            Self::Session(session) => {
                let weak = session.workspace.clone();
                let sid = session.id.clone();
                let item = SidebarMenuItem::new(session.title)
                    .active(session.active)
                    .disable(session.disabled)
                    .when(session.running, |item| {
                        item.suffix(|_, cx| icon_spinner().xsmall().color(cx.theme().primary))
                    })
                    .context_menu({
                        let weak = weak.clone();
                        let sid = sid.clone();
                        move |menu, _, _| {
                            let archive = weak.clone();
                            let archive_id = sid.clone();
                            let delete = weak.clone();
                            let delete_id = sid.clone();
                            menu.item(
                                PopupMenuItem::new(if session.archived {
                                    "取消归档"
                                } else {
                                    "归档会话"
                                })
                                .on_click(move |_, w, cx| {
                                    let _ = archive.update(cx, |this, cx| {
                                        this.manage_session(&archive_id, false, w, cx)
                                    });
                                }),
                            )
                            .item(
                                PopupMenuItem::new("删除会话").on_click(move |_, w, cx| {
                                    let _ = delete.update(cx, |this, cx| {
                                        this.request_delete_session(delete_id.to_string(), w, cx)
                                    });
                                }),
                            )
                        }
                    })
                    .on_click(move |_, w, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if let Some(ix) =
                                this.db.sessions.iter().position(|s| s.id == sid.as_ref())
                            {
                                this.activate(ix, w, cx);
                            }
                        });
                    });
                div()
                    .debug_selector({
                        let id = session.id.clone();
                        move || format!("sidebar-session-{id}")
                    })
                    .id(session.id.clone())
                    .ml_3p5()
                    .pl_2p5()
                    .border_l_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(item.render(session.id, window, cx))
                    .into_any_element()
            }
        }
    }
}

use super::*;
use gpui_kit::component::Collapsible;

// SidebarGroup only accepts a text heading. Keep the hover action and stable
// session identities here; Sidebar still owns scrolling and menu presentation.
#[derive(Clone)]
pub(super) struct ProjectNavigation {
    pub(super) path: omp_core::environment::ProjectLocation,
    pub(super) label: SharedString,
    pub(super) sessions: Vec<(SharedString, SidebarMenuItem)>,
    pub(super) workspace: WeakEntity<Workspace>,
    pub(super) archived: bool,
    pub(super) folded: bool,
    pub(super) disabled: bool,
}

impl Collapsible for ProjectNavigation {
    fn is_collapsed(&self) -> bool {
        false
    }
    fn collapsed(self, _: bool) -> Self {
        self
    }
}

impl SidebarItem for ProjectNavigation {
    fn render(
        self,
        _: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let group_id = SharedString::from(format!(
            "project-{}-{}",
            self.path.environment,
            self.path.path.text()
        ));
        let action_group = group_id.clone();
        let toggle = self.workspace.clone();
        let toggle_path = self.path.clone();
        let folded = self.folded;
        let toggle_label = format!(
            "{}项目 {}",
            if folded { "展开" } else { "收起" },
            self.label
        );
        let weak = self.workspace;
        let path = self.path;
        let empty = self.sessions.is_empty();
        div().id(group_id.clone()).child(
            gpui_kit::component::collapsible::Collapsible::new()
                .open(!folded)
                .gap_1()
                .pb_3()
                .child(
                    div()
                        .group(group_id)
                        .id("project-toggle")
                        .role(Role::Button)
                        .aria_label(toggle_label)
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            let _ = toggle.update(cx, |this, cx| {
                                if !this.collapsed_projects.remove(&toggle_path) {
                                    this.collapsed_projects.insert(toggle_path.clone());
                                }
                                cx.notify();
                            });
                        })
                        .child(
                            Toolbar::new("project-heading")
                                .xsmall()
                                .h(px(32.))
                                .px_2()
                                .content(
                                    div()
                                        .flex()
                                        .flex_1()
                                        .min_w_0()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Icon::new(if folded {
                                                assets::IconName::Folder
                                            } else {
                                                assets::IconName::FolderOpen
                                            })
                                            .size_4()
                                            .text_color(cx.theme().muted_foreground),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_sm()
                                                .child(self.label),
                                        ),
                                )
                                .when(!self.archived, |bar| {
                                    bar.content(
                                        div()
                                            .invisible()
                                            .group_hover(action_group, |d| d.visible())
                                            .child(
                                                Button::new("add-session")
                                                    .ghost()
                                                    .xsmall()
                                                    .icon(IconName::Plus)
                                                    .accessibility_label("新建会话")
                                                    .tooltip("在项目内新建会话")
                                                    .disabled(self.disabled)
                                                    .on_click(move |_, w, cx| {
                                                        cx.stop_propagation();
                                                        let _ = weak.update(cx, |this, cx| {
                                                            this.new_session_at(path.clone(), w, cx)
                                                        });
                                                    }),
                                            ),
                                    )
                                }),
                        ),
                )
                .content(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(self.sessions.into_iter().map(|(id, item)| {
                            div()
                                .pl_3()
                                .child(item.render(id, window, cx))
                                .into_any_element()
                        }))
                        .when(empty, |d| {
                            d.child(
                                div()
                                    .pl_8()
                                    .py_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if self.archived {
                                        "暂无已归档会话"
                                    } else {
                                        "点击 + 开始会话"
                                    }),
                            )
                        }),
                ),
        )
    }
}

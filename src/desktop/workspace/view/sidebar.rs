use super::*;
use gpui_kit::component::sidebar::Sidebar;

mod project;
use project::ProjectNavigation;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = self.save_disabled;
        let groups = self
            .environment_projects()
            .into_iter()
            .map(|project| {
                let mut sessions = self
                    .db
                    .sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        s.project_location() == project
                            && !s.deleted
                            && s.archived == self.show_archived
                    })
                    .collect::<Vec<_>>();
                sessions.sort_by_key(|(_, s)| std::cmp::Reverse(s.updated));
                let sessions = sessions
                    .into_iter()
                    .map(|(ix, s)| {
                        let item = SidebarMenuItem::new(s.title.clone())
                            .active(self.active == Some(ix) && self.settings_view.is_none())
                            .when(self.session_is_running(&s.id), |item| {
                                item.suffix(|_, cx| {
                                    gpui_kit::component::spinner::Spinner::new()
                                        .xsmall()
                                        .color(cx.theme().primary)
                                })
                            })
                            .disable(disabled)
                            .context_menu({
                                let weak = cx.entity().downgrade();
                                let sid = s.id.clone();
                                let archived = s.archived;
                                move |menu, _, _| {
                                    let w1 = weak.clone();
                                    let id1 = sid.clone();
                                    let w2 = weak.clone();
                                    let id2 = sid.clone();
                                    menu.item(
                                        PopupMenuItem::new(if archived {
                                            "取消归档"
                                        } else {
                                            "归档会话"
                                        })
                                        .on_click(
                                            move |_, w, cx| {
                                                let _ = w1.update(cx, |this, cx| {
                                                    this.manage_session(&id1, false, w, cx)
                                                });
                                            },
                                        ),
                                    )
                                    .item(
                                        PopupMenuItem::new("删除会话").on_click(move |_, w, cx| {
                                            let _ = w2.update(cx, |this, cx| {
                                                this.request_delete_session(id2.clone(), w, cx)
                                            });
                                        }),
                                    )
                                }
                            })
                            .on_click(cx.listener(move |this, _, w, cx| this.activate(ix, w, cx)));
                        (SharedString::from(s.id.clone()), item)
                    })
                    .collect();
                ProjectNavigation {
                    path: project.clone(),
                    label: project.label().into(),
                    sessions,
                    workspace: cx.entity().downgrade(),
                    archived: self.show_archived,
                    folded: self.collapsed_projects.contains(&project),
                    disabled,
                }
            })
            .collect::<Vec<_>>();
        Sidebar::new("project-sidebar")
            .collapsible(false)
            .w(px(self.db.sidebar_width))
            .h_full()
            .header(
                // Cancel Sidebar's built-in header inset so all three pane
                // headers share the same full-width 36px baseline.
                div()
                    .flex()
                    .flex_col()
                    .w(px(self.db.sidebar_width - 1.))
                    .mt(rems(-0.75))
                    .mx(rems(-0.75))
                    .child(self.render_environment_selector(cx))
                    .child(
                        div().group("projects-heading").px_3().pt_2().child(
                            Toolbar::new("projects-toolbar")
                                .xsmall()
                                .h(px(32.))
                                .px_2()
                                .content(
                                    div()
                                        .flex_1()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(if self.show_archived {
                                            "已归档项目会话"
                                        } else {
                                            "项目"
                                        }),
                                )
                                .when(!self.show_archived, |bar| {
                                    bar.content(
                                        div()
                                            .invisible()
                                            .group_hover("projects-heading", |d| d.visible())
                                            .child(
                                                Button::new("add-project")
                                                    .ghost()
                                                    .xsmall()
                                                    .icon(IconName::Plus)
                                                    .accessibility_label("添加项目")
                                                    .tooltip("将目录添加为项目")
                                                    .disabled(disabled)
                                                    .on_click(cx.listener(|this, _, w, cx| {
                                                        this.open_project(w, cx)
                                                    })),
                                            ),
                                    )
                                }),
                        ),
                    ),
            )
            .children(groups)
            .footer(
                SidebarMenu::new()
                    .w_full()
                    .gap_1()
                    .border_t_1()
                    .border_color(cx.theme().sidebar_border)
                    .pt_2()
                    .child(
                        SidebarMenuItem::new(if self.show_archived {
                            "返回会话"
                        } else {
                            "已归档会话"
                        })
                        .icon(assets::IconName::Archive)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_archived = !this.show_archived;
                            cx.notify();
                        })),
                    )
                    .child(
                        SidebarMenuItem::new("设置")
                            .icon(IconName::Settings)
                            .active(self.settings_view.is_some())
                            .on_click(cx.listener(|this, _, w, cx| this.settings(w, cx))),
                    )
                    .render("sidebar-actions", window, cx),
            )
            .into_any_element()
    }

    fn render_environment_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::assets::IconName as EnvironmentIcon;

        let environments = self.db.environments.clone();
        let active = self.db.active_environment.clone();
        let label = if active == "local" {
            "本地".into()
        } else {
            format!("SSH：{}", self.environment().name)
        };
        let weak = cx.entity().downgrade();
        let trigger = Button::new("execution-environment")
            .ghost()
            .small()
            // DropdownMenu's popover is a shrink-to-fit flex item. Give its
            // trigger the actual pane width so one hit area spans the row.
            .h(px(PANE_HEADER_HEIGHT - 1.))
            .w(px(self.db.sidebar_width - 1.))
            .rounded_none()
            .px_3()
            .accessibility_label(format!("选择连接：{label}"))
            .disabled(self.save_disabled)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Icon::new(if active == "local" {
                            EnvironmentIcon::Monitor
                        } else {
                            EnvironmentIcon::Server
                        })
                        .small()
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label),
                    ),
            )
            .child(
                Icon::new(EnvironmentIcon::ChevronDown)
                    .small()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground),
            );
        let menu_width = px((self.db.sidebar_width - 16.).max(220.));
        let selector = trigger.dropdown_menu(move |mut menu, _, _| {
            menu = menu.min_w(menu_width);
            for environment in &environments {
                let id = environment.id.clone();
                let weak = weak.clone();
                menu = menu.item(
                    PopupMenuItem::new(environment.name.clone())
                        .icon(Icon::new(if environment.id == "local" {
                            EnvironmentIcon::Monitor
                        } else {
                            EnvironmentIcon::Server
                        }))
                        .checked(active == environment.id)
                        .on_click(move |_, w, cx| {
                            let _ = weak.update(cx, |this, cx| this.switch_environment(&id, w, cx));
                        }),
                );
            }
            menu = menu.separator();
            if active != "local" {
                let weak = weak.clone();
                let active = active.clone();
                menu = menu.item(
                    PopupMenuItem::new("编辑当前连接…")
                        .icon(IconName::Settings)
                        .on_click(move |_, w, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.manage_connection(Some(active.clone()), w, cx)
                            });
                        }),
                );
            }
            let weak = weak.clone();
            menu.item(
                PopupMenuItem::new("新增 SSH 连接…")
                    .icon(IconName::Plus)
                    .on_click(move |_, w, cx| {
                        let _ = weak.update(cx, |this, cx| this.manage_connection(None, w, cx));
                    }),
            )
        });
        Toolbar::new("connection-toolbar")
            .xsmall()
            .h(px(PANE_HEADER_HEIGHT))
            .min_w_0()
            .py_0()
            .px_0()
            .rounded_none()
            .border_b_1()
            .border_color(cx.theme().border)
            .content(selector)
            .into_any_element()
    }
}

use super::*;

impl Workspace {
    pub(super) fn render_sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let disabled = self.running || self.save_disabled;
        let mut items = div()
            .id("projects-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_4()
            .pt_2();
        for project in &self.db.projects {
            let label = project
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path = project.clone();
            let mut group = div().flex().flex_col().gap_1().child(
                div()
                    .group(SharedString::from(format!("project-{}", project.display())))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_8()
                    .child(
                        Icon::new(IconName::Folder)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    )
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(label))
                    .when(!self.show_archived, |row| {
                        row.child(
                            div()
                                .invisible()
                                .group_hover(
                                    SharedString::from(format!("project-{}", project.display())),
                                    |d| d.visible(),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "add-session-{}",
                                        project.display()
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Plus)
                                    .accessibility_label("新建会话")
                                    .tooltip("在项目内新建会话")
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.new_project_session(path.clone(), w, cx)
                                    })),
                                ),
                        )
                    }),
            );
            let mut sessions = self
                .db
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    &s.project == project && !s.deleted && s.archived == self.show_archived
                })
                .collect::<Vec<_>>();
            sessions.sort_by_key(|(_, s)| std::cmp::Reverse(s.updated));
            for (ix, s) in sessions {
                group = group.child(
                    div()
                        .id(SharedString::from(format!("nav-{}", s.id)))
                        .pl_3()
                        .child(
                            SidebarMenu::new()
                                .child(
                                    SidebarMenuItem::new(s.title.clone())
                                        .active(self.active == Some(ix))
                                        .when(self.running && self.active == Some(ix), |item| {
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
                                                    .on_click(move |_, w, cx| {
                                                        let _ = w1.update(cx, |this, cx| {
                                                            this.manage_session(&id1, false, w, cx)
                                                        });
                                                    }),
                                                )
                                                .item(PopupMenuItem::new("删除会话").on_click(
                                                    move |_, w, cx| {
                                                        let _ = w2.update(cx, |this, cx| {
                                                            this.request_delete_session(
                                                                id2.clone(),
                                                                w,
                                                                cx,
                                                            )
                                                        });
                                                    },
                                                ))
                                            }
                                        })
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.activate(ix, w, cx)
                                        })),
                                )
                                .render(SharedString::from(s.id.clone()), window, cx),
                        ),
                );
            }
            if !self
                .db
                .sessions
                .iter()
                .any(|s| &s.project == project && !s.deleted && s.archived == self.show_archived)
            {
                group = group.child(
                    div()
                        .pl_9()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if self.show_archived {
                            "暂无已归档会话"
                        } else {
                            "点击 + 开始会话"
                        }),
                );
            }
            items = items.child(group);
        }
        div()
            .flex()
            .flex_col()
            .w(px(self.db.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .p_3()
            .gap_3()
            .child(
                div().flex().items_center().h_10().px_2().gap_2().child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("OMP"),
                ),
            )
            .child(
                div()
                    .group("projects-heading")
                    .flex()
                    .items_center()
                    .px_2()
                    .h_8()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(if self.show_archived {
                                "已归档项目会话"
                            } else {
                                "项目"
                            }),
                    )
                    .when(!self.show_archived, |row| {
                        row.child(
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
                                        .on_click(
                                            cx.listener(|this, _, w, cx| this.open_project(w, cx)),
                                        ),
                                ),
                        )
                    }),
            )
            .child(items.overflow_y_scrollbar())
            .child(
                Button::new("show-archived")
                    .ghost()
                    .small()
                    .label(if self.show_archived {
                        "返回会话"
                    } else {
                        "已归档会话"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_archived = !this.show_archived;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenu::new()
                    .child(
                        SidebarMenuItem::new("设置")
                            .icon(IconName::Settings)
                            .active(self.settings_view.is_some())
                            .on_click(cx.listener(|this, _, w, cx| this.settings(w, cx))),
                    )
                    .render("sidebar-settings", window, cx),
            )
            .into_any_element()
    }
}

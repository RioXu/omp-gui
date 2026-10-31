use super::*;

impl Render for ResizeSide {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let width = window.viewport_size().width;
        let selection_view = cx.entity().downgrade();
        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .flex()
            .size_full()
            .items_stretch()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_base()
            .child(super::selection::selection_guard(move |_, cx| {
                let _ = selection_view.update(cx, |this, _| this.selection_dirty = true);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selection_dirty = true),
            )
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<ResizeSide>, w, cx| {
                let left = e.drag(cx).0;
                let x: f32 = e.event.position.x.into();
                let width: f32 = w.viewport_size().width.into();
                if left {
                    let panel = if this.panel_open {
                        this.db.panel_width
                    } else {
                        0.
                    };
                    this.db.sidebar_width = x.clamp(170., (width - panel - 360.).clamp(170., 420.));
                } else {
                    let sidebar = if width > 1000. {
                        this.db.sidebar_width
                    } else {
                        0.
                    };
                    this.db.panel_width =
                        (width - x).clamp(280., (width - sidebar - 360.).max(280.));
                }
                this.dirty = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SendPrompt, w, cx| this.send(w, cx)))
            .on_action(
                cx.listener(|this, _: &SaveDocument, w, cx| this.save_active_document(w, cx)),
            )
            .on_action(cx.listener(|this, _: &FindInSession, w, cx| this.open_search(w, cx)))
            .on_action(cx.listener(|this, _: &AcceptCompletion, w, cx| {
                this.accept_completion(this.composer_state.selected_completion, w, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NextCompletion, _, cx| this.move_completion(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousCompletion, _, cx| this.move_completion(true, cx)),
            )
            .on_action(cx.listener(|this, _: &DismissCompletion, _, cx| {
                this.composer_state.dismissed = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextSearchMatch, _, cx| this.move_search(false, cx)))
            .on_action(
                cx.listener(|this, _: &PreviousSearchMatch, _, cx| this.move_search(true, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseSearch, w, cx| this.close_search(w, cx)))
            .on_action(cx.listener(|this, _: &AddComment, w, cx| this.begin_comment(w, cx)))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| {
                if this.panel_open {
                    this.close_panel();
                } else {
                    this.panel_open = true;
                    this.refresh_files(cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewSession, w, cx| this.new_session(w, cx)))
            .on_action(cx.listener(|this, _: &OpenProject, w, cx| this.open_project(w, cx)));
        if width > px(1000.) || !self.panel_open {
            root = root.child(self.render_sidebar(window, cx)).child(
                div().relative().w_0().h_full().flex_shrink_0().child(
                    resize_handle("sidebar-resize", Axis::Horizontal)
                        .on_drag(ResizeSide(true), |d, _, _, cx| cx.new(|_| d.clone())),
                ),
            );
        }
        if let Some(settings) = &self.settings_view {
            return root.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        Toolbar::new("settings-toolbar")
                            .xsmall()
                            .h(px(PANE_HEADER_HEIGHT))
                            .px_3()
                            .border_b_1()
                            .border_color(theme.border)
                            .content(div().flex_1().text_sm().child("设置"))
                            .child(
                                Button::new("close-settings")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Close)
                                    .accessibility_label("返回会话")
                                    .tooltip("返回会话")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings_view = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(settings.clone())),
            );
        }
        let mut center = div().flex().flex_col().h_full().flex_1().min_w_0().child(
            div()
                .flex()
                .items_center()
                .relative()
                .h(px(PANE_HEADER_HEIGHT))
                .flex_shrink_0()
                .gap_1()
                .bg(theme.sidebar)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h_px()
                        .bg(theme.border),
                )
                .child(self.render_document_tabs(cx))
                .child(
                    Toolbar::new("document-actions")
                        .xsmall()
                        .h(px(PANE_HEADER_HEIGHT))
                        .px_1()
                        .child(
                            Button::new("find-session")
                                .ghost()
                                .small()
                                .icon(IconName::Search)
                                .accessibility_label("查找")
                                .tooltip("查找当前标签页 · Ctrl+F")
                                .on_click(cx.listener(|this, _, w, cx| this.open_search(w, cx))),
                        )
                        .child(
                            Button::new("panel-toggle")
                                .ghost()
                                .small()
                                .icon(IconName::PanelRight)
                                .accessibility_label("工作面板")
                                .tooltip("工作面板")
                                .selected(self.panel_open)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.panel_open {
                                        this.close_panel();
                                    } else {
                                        this.panel_open = true;
                                        this.refresh_files(cx);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("theme")
                                .ghost()
                                .small()
                                .icon(IconName::Sun)
                                .accessibility_label("切换深浅色")
                                .tooltip("切换深浅色")
                                .on_click(cx.listener(|this, _, w, cx| this.toggle_theme(w, cx))),
                        ),
                ),
        );
        if self.search_state.open {
            center = center.child(self.render_search(cx));
        }
        if self.document_state.active.is_some() {
            center = center.child(self.render_document(cx));
        } else {
            if let Some(error) = &self.error {
                center = center.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .mx_4()
                        .p_2()
                        .rounded_md()
                        .bg(theme.danger.opacity(0.1))
                        .child(div().flex_1().child(error.clone()))
                        .child(
                            Button::new("dismiss-error")
                                .ghost()
                                .xsmall()
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.error = None;
                                    cx.notify();
                                })),
                        ),
                );
            }
            if self.session().is_some_and(|s| !s.messages.is_empty()) {
                let weak = cx.entity().downgrade();
                center = center.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(
                            list(self.list.clone(), move |ix, _, cx| {
                                weak.update(cx, |this, cx| this.render_message(ix, cx))
                                    .unwrap_or_else(|_| div().into_any_element())
                            })
                            .flex_1()
                            .min_h_0(),
                        )
                        .vertical_scrollbar(&self.list),
                );
            } else {
                center =
                    center.child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .child(div().text_2xl().child(if self.session().is_some() {
                                "描述你想完成的工作。"
                            } else {
                                if self.show_archived {
                                    "已归档会话"
                                } else {
                                    "从这里开始。"
                                }
                            }))
                            .child(div().text_color(theme.muted_foreground).child(
                                if self.session().is_some() {
                                    "任务将在本会话的项目目录中执行。"
                                } else {
                                    if self.show_archived {
                                        "从左侧查看或恢复已归档会话。"
                                    } else {
                                        "添加项目，再点击项目右侧 + 创建会话。"
                                    }
                                },
                            ))
                            .when(!self.show_archived, |d| {
                                d.child(Button::new("empty-open").label("打开项目…").on_click(
                                    cx.listener(|this, _, w, cx| this.open_project(w, cx)),
                                ))
                            }),
                    );
            }
            center = center
                .child(self.render_requests(cx))
                .child(self.render_composer(window, cx));
        }
        root = root.child(center);
        if self.panel_open {
            root = root
                .child(
                    div().relative().w_0().h_full().flex_shrink_0().child(
                        resize_handle("work-panel-resize", Axis::Horizontal)
                            .on_drag(ResizeSide(false), |d, _, _, cx| cx.new(|_| d.clone())),
                    ),
                )
                .child(self.render_panel(cx));
        }
        if self.selected.is_some() || self.comment_editor.is_some() {
            let weak = cx.entity().downgrade();
            let content_weak = weak.clone();
            let open = self.comment_editor.is_some();
            let x = self
                .selection_point
                .x
                .min((width - px(370.)).max(px(8.)))
                .max(px(8.));
            let y = (self.selection_point.y - px(36.)).max(px(50.));
            root = root.child(
                div().absolute().left(x).top(y).child(
                    Popover::new("selection-comment-popover")
                        .appearance(false)
                        .anchor(gpui_kit::Anchor::TopLeft)
                        .open(open)
                        .trigger(
                            Button::new("selection-comment")
                                .small()
                                .bg(theme.popover.opacity(1.0))
                                .icon(IconName::Plus)
                                .label("评论")
                                .tooltip("Ctrl+Alt+M"),
                        )
                        .on_open_change(move |open, w, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if *open {
                                    this.begin_comment(w, cx);
                                } else {
                                    this.comment_editor = None;
                                    this.selected = None;
                                    cx.notify();
                                }
                            });
                        })
                        .content(move |_, _, cx| {
                            content_weak
                                .update(cx, |this, cx| this.render_comment_editor(cx))
                                .unwrap_or_else(|_| div().into_any_element())
                        }),
                ),
            );
        }
        root
    }
}

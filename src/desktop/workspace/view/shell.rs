use super::*;
use gpui_kit::component::TitleBar;

impl Workspace {
    fn window_frame(&self, content: impl IntoElement) -> Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                TitleBar::new().child(
                    div()
                        .debug_selector(|| "workspace-title".into())
                        .text_sm()
                        .child(if self.demo { "OMP · 演示" } else { "OMP" }),
                ),
            )
            .child(content)
    }
}

impl Render for ResizeSide {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let width = window.viewport_size().width;
        let skin = crate::desktop::theme::Skin::from_saved(&self.db.theme, theme.is_dark());
        let skin_view = cx.entity().downgrade();
        let selection_view = cx.entity().downgrade();
        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
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
                cx.listener(|this, _: &PreviousInput, w, cx| {
                    this.browse_input_history(true, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &NextInput, w, cx| this.browse_input_history(false, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SaveDocument, w, cx| this.save_active_document(w, cx)),
            )
            .on_action(cx.listener(|this, _: &FindInSession, w, cx| this.open_search(w, cx)))
            .on_action(cx.listener(|this, _: &AcceptCompletion, w, cx| {
                this.accept_completion(this.runtime.composer_state.selected_completion, w, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NextCompletion, _, cx| this.move_completion(false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousCompletion, _, cx| this.move_completion(true, cx)),
            )
            .on_action(cx.listener(|this, _: &DismissCompletion, _, cx| {
                this.runtime.composer_state.dismissed = true;
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
            return self.window_frame(
                root.child(
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
                ),
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
                        .content(
                            Button::new("theme")
                                .ghost()
                                .small()
                                .icon(assets::IconName::Palette)
                                .accessibility_label(format!("选择皮肤：{}", skin.label()))
                                .tooltip(format!("皮肤 · {}", skin.label()))
                                .dropdown_menu(move |menu, _, _| {
                                    let mut menu = menu.label("皮肤 · 配色与字体");
                                    for option in crate::desktop::theme::Skin::ALL {
                                        let view = skin_view.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(option.label())
                                                .checked(option == skin)
                                                .on_click(move |_, window, cx| {
                                                    let _ = view.update(cx, |this, cx| {
                                                        this.choose_skin(option, window, cx)
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }),
                        ),
                ),
        );
        if self.search_state.open {
            center = center.child(self.render_search(cx));
        }
        if let Some(id) = self.active_subagent() {
            center = center.child(self.render_subagent_transcript(id, cx));
        } else if self.document_state.active.is_some() {
            center = center.child(self.render_document(cx));
        } else {
            if let Some(error) = &self.runtime.error {
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
                                    this.runtime.error = None;
                                    cx.notify();
                                })),
                        ),
                );
            }
            if self.session().is_some_and(|s| !s.messages.is_empty()) {
                let total = self.session().unwrap().messages.len();
                let start = self.runtime.history_start;
                let end = self.runtime.history_end.unwrap_or(total).min(total);
                if total > 100 {
                    center = center.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px(relative(0.1))
                            .py_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "显示第 {}–{} 条 · 共 {} 条（搜索当前页）",
                                start + 1,
                                end,
                                total
                            ))
                            .child(
                                Button::new("history-older")
                                    .ghost()
                                    .xsmall()
                                    .label("更早记录")
                                    .disabled(start == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.show_history_page(Some(start), cx)
                                    })),
                            )
                            .child(
                                Button::new("history-newer")
                                    .ghost()
                                    .xsmall()
                                    .label("较新记录")
                                    .disabled(end == total)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.show_history_page(Some((end + 100).min(total)), cx)
                                    })),
                            )
                            .child(
                                Button::new("history-latest")
                                    .ghost()
                                    .xsmall()
                                    .label("回到最新")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_history_page(None, cx);
                                        this.runtime.list.scroll_to_end();
                                    })),
                            ),
                    );
                }
                let weak = cx.entity().downgrade();
                center = center.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(
                            list(self.runtime.list.clone(), move |ix, window, cx| {
                                weak.update(cx, |this, cx| this.render_message(ix, window, cx))
                                    .unwrap_or_else(|_| div().into_any_element())
                            })
                            .flex_1()
                            .min_h_0(),
                        )
                        .vertical_scrollbar(&self.runtime.list),
                );
            } else {
                center = center.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .px_6()
                        .text_center()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w_16()
                                .h_16()
                                .mb_3()
                                .rounded_2xl()
                                .bg(crate::desktop::theme::surface_sheen(cx))
                                .border_1()
                                .border_color(theme.border)
                                .shadow_sm()
                                .text_color(theme.primary)
                                .text_xl()
                                .font_weight(FontWeight::BOLD)
                                .child("OMP"),
                        )
                        .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(
                            if self.session().is_some() {
                                "描述你想完成的工作。"
                            } else {
                                if self.show_archived {
                                    "已归档会话"
                                } else {
                                    "从这里开始。"
                                }
                            },
                        ))
                        .child(div().text_sm().text_color(theme.muted_foreground).child(
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
                        .when(!self.show_archived && self.session().is_none(), |d| {
                            d.child(
                                Button::new("empty-open")
                                    .primary()
                                    .label("打开项目…")
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.open_project(w, cx)),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .justify_center()
                                .gap_3()
                                .mt_3()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .children(
                                    [
                                        "Ctrl + O  打开项目",
                                        "Ctrl + N  新建会话",
                                        "Ctrl + Enter  发送",
                                    ]
                                    .map(|label| {
                                        div()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.sidebar)
                                            .child(label)
                                    }),
                                ),
                        ),
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
            // Pointer coordinates include the title bar; this overlay is inside the content.
            let y = (self.selection_point.y - px(36.)).max(px(50.))
                - gpui_kit::component::TITLE_BAR_HEIGHT;
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
        self.window_frame(root)
    }
}

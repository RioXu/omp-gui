use super::*;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(self.db.panel_width))
            .min_w(rems(18.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.border)
            .child(
                Toolbar::new("work-panel-toolbar")
                    .xsmall()
                    .px_3()
                    .h(px(PANE_HEADER_HEIGHT))
                    .border_b_1()
                    .border_color(theme.border)
                    .content(div().flex_1().text_sm().child("工作面板"))
                    .child(
                        Button::new("close-panel")
                            .ghost()
                            .small()
                            .icon(IconName::PanelRight)
                            .accessibility_label("收起工作面板")
                            .tooltip("收起工作面板 · Ctrl+Shift+B")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_panel();
                                cx.notify();
                            })),
                    ),
            );
        let labels = ["变更", "文件", "任务", "子智能体"];
        let mut tabs = TabBar::new("work-panel-tabs")
            .underline()
            .with_size(gpui_kit::component::Size::Medium)
            .w_full()
            .flex_shrink_0()
            .selected_index(
                labels
                    .iter()
                    .position(|label| *label == self.panel_tab)
                    .unwrap_or(0),
            );
        for label in labels {
            tabs = tabs.child(
                Tab::new()
                    .selected(self.panel_tab == label)
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_panel(label);
                        this.selected = None;
                        if label == "文件" || label == "变更" {
                            this.refresh_files(cx);
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(div().flex_shrink_0().px_2().child(tabs));
        let mut body = div()
            .id(SharedString::from(format!(
                "work-panel-body-{}",
                self.panel_tab
            )))
            .flex_1()
            .min_h_0()
            .px_1()
            .py_2()
            .flex()
            .flex_col()
            .gap_2();
        match self.panel_tab {
            "任务" => {
                body = body.child(self.render_todos(cx));
            }
            "子智能体" => {
                body = body.child(self.render_subagents(cx));
            }
            "变更" => {
                body = body.px_0().gap_0().child(self.render_changes(cx));
            }
            _ => {
                let paths = &self.document_state.files;
                let tree_state = &self.document_state.tree;
                let weak = cx.entity().downgrade();
                let active_path = self
                    .document_state
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().replace('\\', "/"));
                let changes = self
                    .document_state
                    .changes
                    .iter()
                    .map(|c| (c.path.to_string_lossy().replace('\\', "/"), c.kind))
                    .collect::<std::collections::HashMap<_, _>>();
                body = body.child(
                    div().flex_1().min_h_0().child(
                        Tree::new(tree_state, move |_, entry, _, _, cx| {
                            let folder = entry.is_folder();
                            let path = entry.item().id.to_string();
                            let weak = weak.clone();
                            let kind = changes.get(&path).copied();
                            ListItem::new(entry.item().id.clone())
                                .h_7()
                                .px_0()
                                .py_0()
                                .selected(!folder && active_path.as_deref() == Some(path.as_str()))
                                .rounded_none()
                                .child(
                                    div()
                                        .relative()
                                        .h_full()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .text_sm()
                                        .gap_2()
                                        .pl(px(8. + entry.depth() as f32 * 16.))
                                        .child(
                                            Icon::new(if folder {
                                                IconName::Folder
                                            } else {
                                                IconName::File
                                            })
                                            .size_4()
                                            .text_color(cx.theme().muted_foreground),
                                        )
                                        .children((0..entry.depth()).map(|level| {
                                            div()
                                                .absolute()
                                                .left(px(16. + level as f32 * 16.))
                                                .top_0()
                                                .bottom_0()
                                                .w_px()
                                                .bg(cx.theme().border.opacity(0.55))
                                        }))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .when_some(kind.filter(|_| !folder), |d, kind| {
                                                    d.text_color(change_color(kind, cx))
                                                })
                                                .child(entry.item().label.clone()),
                                        ),
                                )
                                .when(!folder, |row| {
                                    row.on_click(move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            this.open_file(path.clone().into());
                                            cx.notify();
                                        });
                                    })
                                })
                        })
                        .size_full(),
                    ),
                );
                if paths.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("暂无文件变更；也可以切换到文件页查看"),
                    );
                }
            }
        }
        panel = panel.child(if matches!(self.panel_tab, "文件" | "变更") {
            body.into_any_element()
        } else {
            body.overflow_y_scrollbar().into_any_element()
        });
        panel.into_any_element()
    }
}

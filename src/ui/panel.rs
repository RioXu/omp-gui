use super::*;

impl Workspace {
    pub(super) fn render_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(self.db.panel_width))
            .min_w(rems(18.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.background)
            .border_l_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_3()
                    .h_12()
                    .child("工作面板")
                    .child(div().flex_1())
                    .child(
                        Button::new("close-panel")
                            .ghost()
                            .small()
                            .label("收起")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_panel();
                                cx.notify();
                            })),
                    ),
            );
        let mut tabs = div().flex().px_2().gap_1();
        for label in ["变更", "文件", "任务", "子智能体"] {
            tabs = tabs.child(
                Button::new(label)
                    .ghost()
                    .small()
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
        panel = panel.child(tabs);
        let mut body = div()
            .id("work-panel-body")
            .flex_1()
            .min_h_0()
            .px_1()
            .py_2()
            .flex()
            .flex_col()
            .gap_2();
        match self.panel_tab {
            "任务" => {
                if self.todos.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("OMP 尚未创建任务清单"),
                    );
                }
                for phase in &self.todos {
                    body = body.child(phase["name"].as_str().unwrap_or("任务").to_string());
                    for task in phase["tasks"].as_array().into_iter().flatten() {
                        let status = match task["status"].as_str().unwrap_or("") {
                            "completed" => "✓",
                            "in_progress" => "进行中",
                            "blocked" => "受阻",
                            "abandoned" => "取消",
                            _ => "待处理",
                        };
                        body = body.child(div().text_sm().py_2().child(format!(
                            "{status}  {}",
                            task["content"].as_str().unwrap_or("")
                        )));
                    }
                }
            }
            "子智能体" => {
                if self.agents.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("本次会话没有子智能体"),
                    );
                }
                for agent in &self.agents {
                    let name = agent["name"]
                        .as_str()
                        .or_else(|| agent["agent"].as_str())
                        .unwrap_or("子智能体");
                    body = body.child(div().py_2().child(format!(
                        "{name} · {}",
                        agent["status"].as_str().unwrap_or("运行中")
                    )));
                }
            }
            "变更" => {
                if self.changed.is_empty() {
                    body = body.child(
                        div()
                            .p_3()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("暂无文件变更"),
                    );
                } else {
                    let paths = self.changed.clone();
                    body = body.gap_0().child(
                        uniform_list(
                            "changed-files",
                            paths.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|ix| {
                                        let path = paths[ix].clone();
                                        let name = path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned();
                                        let parent = path
                                            .parent()
                                            .filter(|p| !p.as_os_str().is_empty())
                                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                                            .unwrap_or_default();
                                        ListItem::new(SharedString::from(format!(
                                            "change-{}",
                                            path.display()
                                        )))
                                        .accessibility_label(path.to_string_lossy().into_owned())
                                        .h_8()
                                        .rounded_none()
                                        .selected(this.viewed_path.as_ref() == Some(&path))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .min_w_0()
                                                .flex_1()
                                                .child(
                                                    Icon::new(IconName::File)
                                                        .size_4()
                                                        .text_color(cx.theme().muted_foreground),
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().success)
                                                        .flex_shrink_0()
                                                        .child(name),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .min_w_0()
                                                        .truncate()
                                                        .child(parent),
                                                ),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.open_document(path.clone(), true);
                                                cx.notify();
                                            }),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .flex_1()
                        .min_h_0(),
                    );
                }
            }
            _ => {
                let paths = if self.panel_tab == "文件" {
                    &self.files
                } else {
                    &self.changed
                };
                let tree_state = if self.panel_tab == "文件" {
                    &self.file_tree
                } else {
                    &self.change_tree
                };
                let weak = cx.entity().downgrade();
                let active_path = self
                    .viewed_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().replace('\\', "/"));
                let changes = self
                    .changed
                    .iter()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .collect::<std::collections::HashSet<_>>();
                body = body.child(
                    div().flex_1().min_h_0().child(
                        Tree::new(tree_state, move |_, entry, _, _, cx| {
                            let folder = entry.is_folder();
                            let path = entry.item().id.to_string();
                            let weak = weak.clone();
                            let modified = changes.contains(&path);
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
                                                .when(modified && !folder, |d| {
                                                    d.text_color(cx.theme().success)
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

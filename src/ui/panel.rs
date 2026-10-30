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
                                this.panel_open = false;
                                this.selected = None;
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
                        this.panel_tab = label;
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
            .overflow_y_scroll()
            .p_3()
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
            _ => {
                let paths = if self.panel_tab == "文件" {
                    &self.files
                } else {
                    &self.changed
                };
                body = body.child(
                    Button::new("refresh-files")
                        .ghost()
                        .xsmall()
                        .label(if self.loading_files {
                            "正在刷新…"
                        } else {
                            "刷新文件"
                        })
                        .disabled(self.loading_files)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.refresh_files(cx);
                            cx.notify();
                        })),
                );
                let tree_state = if self.panel_tab == "文件" {
                    &self.file_tree
                } else {
                    &self.change_tree
                };
                let weak = cx.entity().downgrade();
                body = body.child(
                    div()
                        .h(if self.panel_text.is_some() {
                            rems(12.)
                        } else {
                            rems(32.)
                        })
                        .min_h(rems(6.))
                        .flex_shrink_0()
                        .child(
                            Tree::new(tree_state, move |_, entry, _, _, cx| {
                                let folder = entry.is_folder();
                                let path = entry.item().id.to_string();
                                let weak = weak.clone();
                                ListItem::new(entry.item().id.clone())
                                    .h_7()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .pl(px(entry.depth() as f32 * 14.))
                                            .child(
                                                Icon::new(if folder {
                                                    if entry.is_expanded() {
                                                        IconName::ChevronDown
                                                    } else {
                                                        IconName::ChevronRight
                                                    }
                                                } else {
                                                    IconName::File
                                                })
                                                .size_4()
                                                .text_color(cx.theme().muted_foreground),
                                            )
                                            .when(folder, |d| {
                                                d.child(Icon::new(IconName::Folder).size_4())
                                            })
                                            .child(entry.item().label.clone()),
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
                if let Some(entry) = &self.panel_text {
                    if let Some(quote) = &self.viewed_quote {
                        body = body.child(
                            div()
                                .id("historical-quote")
                                .max_h_24()
                                .overflow_y_scroll()
                                .p_3()
                                .rounded_md()
                                .bg(theme.muted)
                                .border_l_2()
                                .border_color(theme.primary)
                                .text_sm()
                                .child("评论时选中的片段")
                                .child(
                                    div()
                                        .mt_2()
                                        .font_family("Cascadia Code")
                                        .child(quote.clone()),
                                ),
                        );
                    }
                    body = body.child(
                        div()
                            .pt_3()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if self.historical {
                                "评论时版本 · 原始引用保持不变"
                            } else {
                                "当前读取快照 · 后续修改不会覆盖它"
                            }),
                    );
                    if self.viewed_path.is_some() {
                        body = body.child(
                            div().flex().gap_2().child(
                                Button::new("current-code")
                                    .ghost()
                                    .xsmall()
                                    .label("打开当前代码")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_current();
                                        cx.notify();
                                    })),
                            ),
                        );
                    }
                    body = if let Some(diff) = &self.diff {
                        body.child(
                            div()
                                .h(rems(30.))
                                .min_h(rems(18.))
                                .flex_shrink_0()
                                .border_1()
                                .border_color(theme.border)
                                .rounded_md()
                                .overflow_hidden()
                                .child(Diff::new(diff).size_full()),
                        )
                    } else {
                        body.child(TextView::new(&entry.state))
                    };
                } else if paths.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("暂无文件变更；也可以切换到文件页查看"),
                    );
                }
            }
        }
        panel = panel.child(body);
        panel.into_any_element()
    }
}

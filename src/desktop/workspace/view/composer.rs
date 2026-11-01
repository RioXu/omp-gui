use super::*;
use gpui_kit::component::{progress::ProgressCircle, shimmer::ShimmerText};

impl Workspace {
    pub(in crate::desktop::workspace) fn render_pending(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(s) = self.session() else {
            return div().into_any_element();
        };
        let pending: Vec<_> = s.comments.iter().filter(|c| !c.sent).collect();
        if pending.is_empty() {
            return div().into_any_element();
        }
        let mut box_ = div().flex().flex_col().gap_2().child(
            Button::new("pending-toggle")
                .ghost()
                .small()
                .label(format!(
                    "待发送评论 · {}  {}",
                    pending.len(),
                    if self.pending_open { "⌃" } else { "⌄" }
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pending_open = !this.pending_open;
                    cx.notify();
                })),
        );
        if self.pending_open {
            let mut list = div()
                .id("pending-comments")
                .max_h(rems(12.))
                .flex()
                .flex_col()
                .gap_2();
            for c in pending {
                let cid = c.id.clone();
                let edit = c.clone();
                list = list.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(c.anchor.label()),
                        )
                        .child(c.text.clone())
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new(SharedString::from(format!("edit-{cid}")))
                                        .ghost()
                                        .xsmall()
                                        .label("编辑")
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.comment_editor = Some(edit.anchor.clone());
                                            this.editing_comment = Some(edit.id.clone());
                                            this.selection_point = w.mouse_position();
                                            this.comment_input.update(cx, |i, cx| {
                                                i.set_value(edit.text.clone(), w, cx)
                                            });
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("delete-{cid}")))
                                        .ghost()
                                        .xsmall()
                                        .label("删除")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(s) = this.session_mut() {
                                                s.comments.retain(|c| c.id != cid || c.sent);
                                            }
                                            this.persist();
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
            }
            box_ = box_.child(list.overflow_y_scrollbar());
        }
        box_.into_any_element()
    }
    pub(in crate::desktop::workspace) fn render_composer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let paste_view = cx.entity().downgrade();
        let models = crate::desktop::workspace::runtime::model_controls::model_groups(
            &self.runtime.model_controls.available,
        );
        let selected_model = crate::desktop::workspace::runtime::model_controls::model_key(
            &self.runtime.model_controls.current,
        );
        let provider = self.runtime.model_controls.current["provider"]
            .as_str()
            .unwrap_or("");
        let model_label = if provider.is_empty() {
            self.runtime.model_controls.label.clone()
        } else {
            format!("{provider} / {}", self.runtime.model_controls.label)
        };
        let weak = cx.entity().downgrade();
        let model = Button::new("model-picker")
            .ghost()
            .small()
            .label(model_label)
            .disabled(
                self.runtime.running
                    || !self.runtime.connected
                    || models.is_empty()
                    || self.runtime.model_controls.pending.is_some(),
            )
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.scrollable(true);
                for (provider, models) in &models {
                    menu = menu.label(provider.clone());
                    for m in models {
                        let provider = provider.clone();
                        let model = m["id"].as_str().unwrap_or_default().to_owned();
                        let name = m["name"].as_str().unwrap_or(&model);
                        let label = if name == model {
                            model.clone()
                        } else {
                            format!("{name} · {model}")
                        };
                        let checked = selected_model == format!("{provider}/{model}");
                        let weak = weak.clone();
                        menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.choose_model(&provider, &model, cx)
                                });
                            },
                        ));
                    }
                }
                menu
            });
        let levels = self.thinking_options();
        let selected_level = if self.runtime.model_controls.auto {
            "auto".to_owned()
        } else {
            self.runtime.model_controls.thinking.clone()
        };
        let weak = cx.entity().downgrade();
        let thinking = Button::new("thinking")
            .ghost()
            .small()
            .label(format!("推理：{selected_level}"))
            .tooltip(format!(
                "当前模型的有效推理强度：{}",
                self.runtime.model_controls.thinking
            ))
            .disabled(
                self.runtime.running
                    || !self.runtime.connected
                    || self.runtime.model_controls.pending.is_some()
                    || !levels.iter().any(|l| l != "off"),
            )
            .dropdown_menu(move |mut menu, _, _| {
                for level in &levels {
                    let weak = weak.clone();
                    let level = level.clone();
                    let label = if level == "auto" {
                        "auto · 自动选择".to_owned()
                    } else {
                        level.clone()
                    };
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(level == selected_level)
                            .on_click(move |_, _, cx| {
                                let _ =
                                    weak.update(cx, |this, cx| this.choose_thinking(&level, cx));
                            }),
                    );
                }
                menu
            });
        let mut area = div().flex().flex_col().gap_2().px_6().pb_4().pt_2();
        let mut inner = div()
            .when(!self.completions(cx).is_empty(), |d| {
                d.key_context("ComposerCompletion")
            })
            .when(
                self.completions(cx).is_empty() && self.completion_hint(cx).is_some(),
                |d| d.key_context("ComposerHint"),
            )
            .flex()
            .flex_col()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .gap_2()
            .child(self.render_pending(cx));
        if let Some(cid) = &self.runtime.follow_up {
            inner = inner.child(
                div()
                    .flex()
                    .gap_2()
                    .text_xs()
                    .child(format!("跟进评论 {}", &cid[..8]))
                    .child(
                        Button::new("cancel-follow")
                            .ghost()
                            .xsmall()
                            .label("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.runtime.follow_up = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        for (ix, wire) in self.runtime.queued_inputs.iter().enumerate() {
            let message = wire.clone();
            let text = self
                .runtime
                .submitted_inputs
                .iter()
                .find(|e| &e.wire_text == wire)
                .map(|e| e.text.as_str())
                .unwrap_or(wire);
            inner = inner.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(theme.accent)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("等待本轮完成"),
                            )
                            .child(div().text_sm().truncate().child(text.to_string())),
                    )
                    .child(
                        Button::new(SharedString::from(format!("steer-queued-{ix}")))
                            .small()
                            .label("指示")
                            .disabled(
                                self.runtime.stopping
                                    || self.runtime.steering_request.is_some()
                                    || self.runtime.pending_send.is_some(),
                            )
                            .tooltip("转为 OMP steering 指示；接收时机遵循 OMP")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.steer_queued(message.clone(), cx)
                            })),
                    ),
            );
        }
        for wire in &self.runtime.steered_inputs {
            let text = self
                .runtime
                .submitted_inputs
                .iter()
                .find(|e| &e.wire_text == wire)
                .map(|e| e.text.as_str())
                .unwrap_or(wire);
            inner = inner.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(theme.accent)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("指示已提交给 OMP"),
                    )
                    .child(div().text_sm().truncate().child(text.to_string())),
            );
        }
        inner = inner
            .child(self.render_completions(window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_xl()
                    .bg(theme.muted)
                    .border_1()
                    .border_color(theme.border)
                    .drag_over::<ExternalPaths>(|style, _, _, cx| {
                        style.border_color(cx.theme().primary)
                    })
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        this.import_attachments(
                            paths
                                .0
                                .iter()
                                .cloned()
                                .map(crate::infrastructure::filesystem::attachments::Source::File)
                                .collect(),
                            cx,
                        );
                    }))
                    .child(self.render_draft_attachments(cx))
                    .child(
                        Textarea::new(&self.runtime.composer_state.input)
                            .disabled(self.creating_session() || self.runtime.preparing.is_some())
                            .appearance(false)
                            .aria_label("给 OMP 的指令")
                            .on_paste(move |item, _, cx| {
                                paste_view
                                    .update(cx, |this, cx| this.paste_attachments(item, cx))
                                    .unwrap_or(false)
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(model)
                            .child(thinking)
                            .child(div().flex_1())
                            .when(self.runtime.running, |row| {
                                row.child(
                                    Button::new("stop")
                                        .small()
                                        .label(if self.runtime.stopping {
                                            "正在停止…"
                                        } else {
                                            "停止"
                                        })
                                        .disabled(self.runtime.stopping)
                                        .tooltip("停止生成，并取回未执行的追加指令")
                                        .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
                                )
                            })
                            .child(
                                Button::new("send")
                                    .primary()
                                    .small()
                                    .icon(IconName::ArrowUp)
                                    .label(if self.runtime.running {
                                        "追加"
                                    } else {
                                        "发送"
                                    })
                                    .disabled(
                                        self.runtime.pending_send.is_some()
                                            || self.runtime.preparing.is_some()
                                            || self.creating_session()
                                            || self.runtime.steering_request.is_some()
                                            || self.runtime.stopping
                                            || self.importing_attachments()
                                            || (self
                                                .runtime
                                                .composer_state
                                                .input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .is_empty()
                                                && !self.session().is_some_and(|s| {
                                                    !s.draft_attachments.is_empty()
                                                        || s.comments.iter().any(|c| !c.sent)
                                                })),
                                    )
                                    .tooltip(if self.runtime.running {
                                        "Ctrl+Enter · 等待本轮完成后发送"
                                    } else {
                                        "Ctrl+Enter"
                                    })
                                    .on_click(cx.listener(|this, _, w, cx| this.send(w, cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if self.demo {
                        div().child("演示模式，不调用模型").into_any_element()
                    } else if self.runtime.running
                        || self.runtime.starting
                        || self.runtime.reconnecting
                        || self.runtime.stopping
                        || self.runtime.preparing.is_some()
                        || self.creating_session()
                        || self.connecting_environment()
                    {
                        ShimmerText::new(self.runtime.status.clone()).into_any_element()
                    } else {
                        div().child(self.runtime.status.clone()).into_any_element()
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("todo-shortcut")
                            .ghost()
                            .xsmall()
                            .label("任务")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.select_panel("任务");
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("context-usage")
                            .tooltip({
                                let label = context_label(
                                    self.runtime.context_used,
                                    self.runtime.context_limit,
                                );
                                move |w, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(label.clone())
                                        .build(w, cx)
                                }
                            })
                            .child(
                                ProgressCircle::new("context-circle")
                                    .with_size(px(18.))
                                    .value(if self.runtime.context_percent.is_finite() {
                                        self.runtime.context_percent as f32
                                    } else {
                                        0.
                                    })
                                    .color(if self.runtime.context_percent >= 90. {
                                        theme.danger
                                    } else {
                                        theme.primary
                                    })
                                    .accessibility_label(context_label(
                                        self.runtime.context_used,
                                        self.runtime.context_limit,
                                    )),
                            ),
                    ),
            );
        area = area.child(inner);
        area.into_any_element()
    }
}

fn context_label(used: Option<u64>, limit: u64) -> String {
    format!(
        "上下文：已用 {} / 总量 {} tokens",
        used.map(|v| v.to_string()).unwrap_or_else(|| "未知".into()),
        if limit > 0 {
            limit.to_string()
        } else {
            "未知".into()
        }
    )
}

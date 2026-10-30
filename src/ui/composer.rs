use super::*;

impl Workspace {
    pub(super) fn render_pending(&self, cx: &mut Context<Self>) -> AnyElement {
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
                .overflow_y_scroll()
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
            box_ = box_.child(list);
        }
        box_.into_any_element()
    }
    pub(super) fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let models = self.models.clone();
        let weak = cx.entity().downgrade();
        let model=Button::new("model-picker").ghost().small().label(self.model_label.clone()).disabled(self.running||models.is_empty()).dropdown_menu(move|mut menu,_,_|{for m in &models{let provider=m["provider"].as_str().unwrap_or("").to_string();let model=m["id"].as_str().unwrap_or("").to_string();let label=format!("{provider}/{model}");let weak=weak.clone();menu=menu.item(PopupMenuItem::new(label).on_click(move|_,_,cx|{let _=weak.update(cx,|this,cx|{this.command(json!({"type":"set_model","id":id(),"provider":provider,"modelId":model}));cx.notify();});}));}menu});
        let weak = cx.entity().downgrade();
        let thinking = Button::new("thinking")
            .ghost()
            .small()
            .label(format!("推理：{}", self.thinking))
            .disabled(self.running || !self.connected)
            .dropdown_menu(move |mut menu, _, _| {
                for level in ["off", "low", "medium", "high", "xhigh"] {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(level).on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.command(
                                json!({"type":"set_thinking_level","id":id(),"level":level}),
                            );
                            cx.notify();
                        });
                    }));
                }
                menu
            });
        let mut area = div().flex().flex_col().gap_2().px_6().pb_4().pt_2();
        let mut inner = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .gap_2()
            .child(self.render_pending(cx));
        if let Some(cid) = &self.follow_up {
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
                                this.follow_up = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        inner = inner
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
                    .child(
                        Textarea::new(&self.input)
                            .appearance(false)
                            .aria_label("给 OMP 的指令"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(model)
                            .child(thinking)
                            .child(div().flex_1())
                            .child(if self.running {
                                Button::new("stop")
                                    .small()
                                    .label("停止")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.command(json!({"type":"abort","id":id()}));
                                        this.status = "正在停止…".into();
                                        cx.notify();
                                    }))
                            } else {
                                Button::new("send")
                                    .primary()
                                    .small()
                                    .icon(IconName::ArrowUp)
                                    .label("发送")
                                    .tooltip("Ctrl+Enter")
                                    .on_click(cx.listener(|this, _, w, cx| this.send(w, cx)))
                            }),
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
                        "演示模式，不调用模型".into()
                    } else {
                        self.status.clone()
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("todo-shortcut")
                            .ghost()
                            .xsmall()
                            .label("任务")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel_open = true;
                                this.panel_tab = "任务";
                                cx.notify();
                            })),
                    )
                    .child(format!("上下文 {:.0}%", self.context_percent)),
            );
        area = area.child(inner);
        area.into_any_element()
    }
}

use super::*;

impl Workspace {
    pub(super) fn render_comment(&self, c: &Comment, cx: &mut Context<Self>) -> AnyElement {
        let cid = c.id.clone();
        let a = c.anchor.clone();
        let reply = cid.clone();
        let resolve = cid.clone();
        let theme = cx.theme().clone();
        let mut actions = div().flex().gap_2().flex_wrap().child(
            Button::new(SharedString::from(format!("anchor-{cid}")))
                .ghost()
                .xsmall()
                .label(if matches!(a.source, Source::File { .. }) {
                    "查看当时变更"
                } else {
                    "定位原文"
                })
                .on_click(cx.listener(move |this, _, _, cx| this.open_anchor(a.clone(), cx))),
        );
        if let Source::File { path, .. } = &c.anchor.source {
            let path = path.clone();
            actions = actions.child(
                Button::new(SharedString::from(format!("current-{cid}")))
                    .ghost()
                    .xsmall()
                    .label("打开当前代码")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.viewed_path = Some(path.clone());
                        this.panel_open = true;
                        this.panel_tab = "变更";
                        this.open_current();
                        cx.notify();
                    })),
            );
        }
        let info = if c.resolved {
            "你已标记解决"
        } else if c.reply_ids.is_empty() {
            "待回应"
        } else {
            "已有跟进 · 待确认"
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_2()
            .mb_3()
            .p_3()
            .bg(theme.muted)
            .rounded_lg()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} · {}", c.anchor.label(), &cid[..8])),
            )
            .child(TextView::markdown(
                SharedString::from(format!("quote-{cid}")),
                format!("> {}", c.anchor.quote.replace('\n', "\n> ")),
            ))
            .child(c.text.clone())
            .child(actions)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(info)
                    .child(
                        Button::new(SharedString::from(format!("follow-{cid}")))
                            .ghost()
                            .xsmall()
                            .label("继续跟进")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.follow_up = Some(reply.clone());
                                this.input.focus_handle(cx).focus(w, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("resolve-{cid}")))
                            .ghost()
                            .xsmall()
                            .label(if c.resolved {
                                "重新打开"
                            } else {
                                "标记已解决"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.session_mut()
                                    && let Some(c) = s.comments.iter_mut().find(|c| c.id == resolve)
                                {
                                    c.resolved = !c.resolved;
                                }
                                this.persist();
                                this.invalidate_list();
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
    pub fn render_message(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.session() else {
            return div().into_any_element();
        };
        let Some(m) = s.messages.get(ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let mut content = div()
            .group("message")
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .py_3();
        let label = if m.role == "user" { "你" } else { "⌥ OMP" };
        content = content.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        );
        for block in &m.process {
            let key = format!("process-{}-{}", m.id, block.id);
            let label = if block.streaming {
                "正在思考…".to_string()
            } else if block.kind == ProcessKind::Thinking {
                "思考过程".to_string()
            } else {
                format!("工具调用 · {}", block.label)
            };
            let mut detail = div()
                .flex()
                .flex_col()
                .gap_2()
                .text_sm()
                .child(block.text.clone());
            if let Some(result) = &block.result {
                detail = detail.child(result.clone());
            }
            content = content.child(self.process_section(key, label, block.streaming, detail, cx));
        }
        if !m.text.trim().is_empty()
            && let Some(view) = self.texts.get(&m.id)
        {
            let row = div()
                .w_full()
                .when(m.role == "user", |d| d.bg(theme.muted).rounded_lg().p_3())
                .child(TextView::new(&view.state));
            let intermediate = m.role == "assistant"
                && (m.process.iter().any(|b| b.kind == ProcessKind::Tool)
                    || s.messages[ix + 1..]
                        .iter()
                        .take_while(|m| m.role != "user")
                        .any(|m| m.role == "assistant"));
            content = if intermediate {
                content.child(self.process_section(
                    format!("answer-{}", m.id),
                    "工作过程".into(),
                    false,
                    row,
                    cx,
                ))
            } else {
                content.child(row)
            };
        }
        for cid in &m.comments {
            if let Some(c) = s.comments.iter().find(|c| &c.id == cid) {
                content = content.child(self.render_comment(c, cx));
            }
        }
        if m.role == "assistant" {
            for c in s.comments.iter().filter(|c| c.reply_ids.contains(&m.id)) {
                let a = c.anchor.clone();
                content = content.child(
                    Button::new(SharedString::from(format!("reply-{}-{}", m.id, c.id)))
                        .ghost()
                        .xsmall()
                        .label(format!("↗ 回应评论 {}", &c.id[..8]))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open_anchor(a.clone(), cx)),
                        ),
                );
            }
        }
        let time = Local
            .timestamp_opt(m.time as i64, 0)
            .single()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        content = content.child(
            div().flex().justify_end().h_4().child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .opacity(0.)
                    .group_hover("message", |style| style.opacity(1.))
                    .child(time),
            ),
        );
        div().w_full().px_6().child(content).into_any_element()
    }
}

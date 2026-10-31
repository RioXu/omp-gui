use super::*;

impl Workspace {
    pub fn render_message(&self, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(entry) = self.runtime.transcript.get(row) else {
            return div().into_any_element();
        };
        if let TranscriptRow::Notice(ix) = entry {
            let message = &session.messages[*ix];
            let mut card = div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .max_w(rems(52.))
                .mx_auto()
                .my_3()
                .p_3()
                .border_l_2()
                .border_color(cx.theme().primary)
                .bg(cx.theme().muted)
                .rounded_md()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.label.clone().unwrap_or_else(|| "会话消息".into())),
                );
            if let Some(view) = self.runtime.texts.get(&message.id) {
                card = card.child(self.message_file_link_view(
                    TextView::new(&view.state),
                    &message.id,
                    cx,
                ));
            }
            for id in &message.comments {
                if let Some(comment) = session.comments.iter().find(|c| &c.id == id) {
                    card = card.child(self.render_comment(comment, cx));
                }
            }
            return div().w_full().px_6().child(card).into_any_element();
        }
        let theme = cx.theme().clone();
        let (range, user) = match entry {
            TranscriptRow::User(ix) => (*ix..ix + 1, true),
            TranscriptRow::Assistant(range) => (range.clone(), false),
            TranscriptRow::Notice(_) => unreachable!(),
        };
        let messages = session.messages[range.clone()]
            .iter()
            .filter(|m| m.state != MessageState::Queued)
            .collect::<Vec<_>>();
        let last = messages.last().unwrap();
        let active = !user
            && self.runtime.running
            && (messages.iter().any(|m| {
                m.process
                    .iter()
                    .any(|b| self.runtime.active_tools.contains_key(&b.id))
            }) || session
                .messages
                .iter()
                .rposition(|m| m.state != MessageState::Queued)
                .is_some_and(|last| range.contains(&last)));
        let mut body = div()
            .group("message")
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .py_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child(if user { "你" } else { "OMP" }),
            );
        if last.state != MessageState::Normal {
            body = body.child(div().text_xs().text_color(theme.muted_foreground).child(
                match last.state {
                    MessageState::Cancelled => "未执行 · 已恢复到草稿",
                    MessageState::Interrupted => "已中断 · 保留已生成内容",
                    MessageState::Normal => "",
                    MessageState::Queued => "等待接收",
                    MessageState::Unconfirmed => "执行状态未确认 · 草稿已恢复，请核对后再发",
                },
            ));
        }
        if !user {
            let show_process = active
                || self
                    .runtime
                    .expanded_process
                    .contains(&format!("turn-{}", messages[0].id));
            let mut process = div().flex().flex_col().gap_1();
            let mut steps = 0;
            for message in &messages {
                for block in &message.process {
                    if show_process {
                        process = process.child(self.render_process_block(&message.id, block, cx));
                    }
                    steps += 1;
                }
                let intermediate = message.id != last.id
                    || message.process.iter().any(|b| b.kind == ProcessKind::Tool);
                if intermediate
                    && !message.text.is_empty()
                    && let Some(view) = self.runtime.texts.get(&message.id)
                {
                    if show_process {
                        process =
                            process.child(div().px_2().py_2().child(self.message_file_link_view(
                                TextView::new(&view.state),
                                &message.id,
                                cx,
                            )));
                    }
                    steps += 1;
                }
            }
            if steps > 0 {
                body = body.child(self.process_section(
                    format!("turn-{}", messages[0].id),
                    if active {
                        format!("正在处理 · {steps} 项过程")
                    } else {
                        format!("过程记录 · {steps} 项")
                    },
                    active,
                    process,
                    cx,
                ));
            }
        }
        let has_answer = user || !last.process.iter().any(|b| b.kind == ProcessKind::Tool);
        if has_answer
            && !last.text.is_empty()
            && let Some(view) = self.runtime.texts.get(&last.id)
        {
            body = body.child(
                div()
                    .w_full()
                    .when(user, |d| d.bg(theme.muted).rounded_lg().px_4().py_3())
                    .child(self.message_file_link_view(TextView::new(&view.state), &last.id, cx)),
            );
        }
        for message in &messages {
            if !message.attachments.is_empty() {
                body = body.child(self.render_attachments(
                    &message.attachments,
                    false,
                    &message.id,
                    cx,
                ));
            }
            for id in &message.comments {
                if let Some(comment) = session.comments.iter().find(|c| &c.id == id) {
                    body = body.child(self.render_comment(comment, cx));
                }
            }
        }
        if !user {
            for comment in session
                .comments
                .iter()
                .filter(|c| messages.iter().any(|m| c.reply_ids.contains(&m.id)))
            {
                let anchor = comment.anchor.clone();
                body = body.child(
                    div().flex().child(
                        Button::new(SharedString::from(format!(
                            "reply-{}-{}",
                            last.id, comment.id
                        )))
                        .ghost()
                        .xsmall()
                        .label(format!("回应评论 {}", &comment.id[..8]))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open_anchor(anchor.clone(), cx)),
                        ),
                    ),
                );
            }
        }
        let time = Local
            .timestamp_opt(last.time as i64, 0)
            .single()
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        body = body.child(
            div().flex().justify_end().h_4().child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .opacity(0.)
                    .group_hover("message", |s| s.opacity(1.))
                    .child(time),
            ),
        );
        div().w_full().px_6().child(body).into_any_element()
    }
}

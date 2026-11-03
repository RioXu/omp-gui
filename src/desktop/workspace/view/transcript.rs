use super::*;
use gpui_kit::base::ElementExt;

impl Workspace {
    pub fn render_message(&self, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(entry) = self.runtime.transcript.get(row) else {
            return div().into_any_element();
        };
        if let TranscriptRow::Notice(ix) = entry {
            return transcript_row(
                self.render_notice(&session.messages[*ix], cx),
                row,
                self.runtime.list.clone(),
            );
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
            .debug_selector(|| "conversation-message-body".into())
            .group("message")
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
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
                    .debug_selector(|| "conversation-answer".into())
                    .w_full()
                    .when(user, |d| {
                        d.bg(crate::desktop::theme::surface_sheen(cx))
                            .rounded_xl()
                            .border_1()
                            .border_color(theme.border)
                            .px_4()
                            .py_3()
                    })
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
        transcript_row(body, row, self.runtime.list.clone())
    }
}

/// GPUI 0.3.10 retries List prepaint for child autoscroll, but its transaction
/// does not roll back accessibility nodes. Consume the native reveal request
/// after the row is laid out and adjust the scroll anchor on the next frame.
/// This preserves native text range/highlight handling without a duplicate
/// accessibility tree (or disabling accessibility).
fn transcript_row(body: impl IntoElement, row: usize, list: ListState) -> AnyElement {
    div()
        .w_full()
        .px(relative(0.1))
        .child(body)
        .on_prepaint(move |bounds, window, cx| {
            let Some(target) = window.take_autoscroll() else {
                return;
            };
            let viewport = window.content_mask().bounds;
            let above = target.top() < viewport.top();
            let below = target.bottom() > viewport.bottom();
            if !above && !below {
                return;
            }
            let offset = if above || target.size.height > viewport.size.height {
                target.top() - bounds.top()
            } else {
                target.bottom() - bounds.top() - viewport.size.height
            }
            .max(px(0.));
            window.defer(cx, move |window, _| {
                if row >= list.item_count() {
                    return;
                }
                let target = ListOffset {
                    item_ix: row,
                    offset_in_item: offset,
                };
                if list.logical_scroll_top() != target {
                    list.scroll_to(target);
                    window.refresh();
                }
            });
        })
        .into_any_element()
}

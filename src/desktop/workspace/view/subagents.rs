use super::super::runtime::collaboration::{subagent_role, subagent_title};
use super::*;
use crate::domain::activity::status_label;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_subagents(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().flex().flex_col().gap_1().px_2();
        if self.runtime.agents.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("本次会话没有子智能体"),
            );
        }
        for agent in &self.runtime.agents {
            let id = agent["id"].as_str().unwrap_or_default().to_owned();
            let role = subagent_role(agent).to_owned();
            let title = subagent_title(agent).to_owned();
            let status = status_label(agent["status"].as_str().unwrap_or("pending")).to_owned();
            let tooltip = title.clone();
            body = body.child(
                ListItem::new(SharedString::from(format!("subagent-{id}")))
                    .accessibility_label(format!("{role}，{status}，{title}"))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("subagent-row-{id}")
                    })
                    .selected(self.active_subagent() == Some(id.as_str()))
                    .px_2()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .w_full()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_xs()
                                    .child(div().flex_1().min_w_0().truncate().child(role))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(status),
                                    ),
                            )
                            .child(
                                div()
                                    .id("subagent-title")
                                    .text_sm()
                                    .truncate()
                                    .tooltip(move |w, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                            .build(w, cx)
                                    })
                                    .child(title),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_subagent(id.clone());
                        cx.notify();
                    })),
            );
        }
        body.into_any_element()
    }

    pub(in crate::desktop::workspace) fn render_subagent_transcript(
        &self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.runtime.collaboration;
        let agent = self.runtime.agents.iter().find(|agent| agent["id"] == id);
        let title = agent.map(subagent_title).unwrap_or("子会话");
        let role = agent.map(subagent_role).unwrap_or("子智能体");
        let mut body = div()
            .id(SharedString::from(format!("subagent-transcript-{id}")))
            .debug_selector({
                let id = id.to_owned();
                move || format!("subagent-transcript-{id}")
            })
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .px(relative(0.1))
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title.to_owned()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · {} · 只读",
                                role,
                                status_label(
                                    agent
                                        .and_then(|a| a["status"].as_str())
                                        .unwrap_or("pending")
                                )
                            )),
                    ),
            );
        if let Some(error) = &state.error {
            body = body.child(
                div()
                    .px(relative(0.1))
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        if let Some(error) = agent
            .and_then(|a| a["error"].as_str())
            .filter(|s| !s.is_empty())
        {
            body = body.child(
                div()
                    .px(relative(0.1))
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.to_owned()),
            );
        }
        let total = state.messages.len();
        let end = state.page_end.unwrap_or(total).min(total);
        if total > 100 {
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .px(relative(0.1))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "第 {}–{} 条 · 共 {} 条",
                        end.saturating_sub(100) + 1,
                        end,
                        total
                    ))
                    .child(
                        Button::new("child-older")
                            .ghost()
                            .xsmall()
                            .label("更早记录")
                            .disabled(end <= 100)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.runtime.collaboration.page_end = Some(end.saturating_sub(100));
                                this.sync_subagent_view(cx);
                                this.runtime.collaboration.list.scroll_to(ListOffset {
                                    item_ix: 0,
                                    offset_in_item: px(0.),
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("child-newer")
                            .ghost()
                            .xsmall()
                            .label("较新记录")
                            .disabled(end == total)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.runtime.collaboration.page_end = Some((end + 100).min(total));
                                this.sync_subagent_view(cx);
                                this.runtime.collaboration.list.scroll_to(ListOffset {
                                    item_ix: 0,
                                    offset_in_item: px(0.),
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("child-latest")
                            .ghost()
                            .xsmall()
                            .label("回到最新")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.runtime.collaboration.page_end = None;
                                this.sync_subagent_view(cx);
                                this.runtime.collaboration.list.scroll_to_end();
                                cx.notify();
                            })),
                    ),
            );
        }
        if state.messages.is_empty() {
            let mut empty = div()
                .id("child-empty")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .px(relative(0.1))
                .py_3();
            if let Some(task) = agent
                .and_then(|a| a["task"].as_str().or_else(|| a["assignment"].as_str()))
                .filter(|s| !s.is_empty() && *s != title)
            {
                empty = empty.child(div().py_2().child(self.file_link_view(
                    crate::desktop::markdown::view(
                        SharedString::from(format!("child-{id}-task")),
                        task,
                    ),
                    cx,
                )));
            }
            let output = agent
                .and_then(|a| a["output"].as_str())
                .filter(|s| !s.is_empty());
            body = body.child(empty.child(self.file_link_view(
                crate::desktop::markdown::view(
                    SharedString::from(format!("child-{id}-empty")),
                    output.unwrap_or(if state.loading {
                        "正在读取子会话…"
                    } else {
                        "子会话暂未写入消息"
                    }),
                ),
                cx,
            )));
        } else {
            let weak = cx.entity().downgrade();
            body = body.child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(
                        list(state.list.clone(), move |row, _, cx| {
                            weak.update(cx, |this, cx| this.render_child_row(row, cx))
                                .unwrap_or_else(|_| div().into_any_element())
                        })
                        .flex_1()
                        .min_h_0(),
                    )
                    .vertical_scrollbar(&state.list),
            );
        }
        body.into_any_element()
    }

    fn render_child_row(&self, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.runtime.collaboration;
        let Some(entry) = state.rows.get(row) else {
            return div().into_any_element();
        };
        let (range, user) = match entry {
            TranscriptRow::User(ix) => (*ix..ix + 1, true),
            TranscriptRow::Notice(ix) => (*ix..ix + 1, false),
            TranscriptRow::Assistant(range) => (range.clone(), false),
        };
        let messages = &state.messages[range];
        let Some(last) = messages.last() else {
            return div().into_any_element();
        };
        let agent = self
            .runtime
            .agents
            .iter()
            .find(|a| Some(a["id"].as_str().unwrap_or("")) == state.selected.as_deref());
        let role = agent.map(subagent_role).unwrap_or("子智能体");
        let key = format!(
            "child-turn-{}-{}",
            state.selected.as_deref().unwrap_or(""),
            messages[0].id
        );
        let expanded = self.runtime.expanded_process.contains(&key);
        let mut body = div()
            .debug_selector(|| "child-message-body".into())
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .py_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child(if user {
                        "任务输入".to_owned()
                    } else {
                        last.label.clone().unwrap_or_else(|| role.to_owned())
                    }),
            );
        if !user {
            let mut process = div().flex().flex_col().gap_1();
            let mut steps = 0;
            for message in messages {
                for block in &message.process {
                    steps += 1;
                    if expanded {
                        process = process.child(self.render_process_block(
                            &format!("child-{}", message.id),
                            block,
                            cx,
                        ));
                    }
                }
                if (message.id != last.id
                    || message.process.iter().any(|b| b.kind == ProcessKind::Tool))
                    && !message.text.is_empty()
                {
                    steps += 1;
                    if expanded && let Some((_, view)) = state.texts.get(&message.id) {
                        process = process.child(self.file_link_view(TextView::new(view), cx));
                    }
                }
            }
            if steps > 0 {
                body = body.child(self.process_section(
                    key,
                    format!("过程记录 · {steps} 项"),
                    false,
                    process,
                    cx,
                ));
            }
        }
        if (user || !last.process.iter().any(|b| b.kind == ProcessKind::Tool))
            && let Some((text, view)) = state.texts.get(&last.id)
            && !text.is_empty()
        {
            body = body.child(
                div()
                    .w_full()
                    .when(user, |d| {
                        d.bg(crate::desktop::theme::surface_sheen(cx))
                            .rounded_xl()
                            .border_1()
                            .border_color(cx.theme().border)
                            .px_4()
                            .py_3()
                    })
                    .child(self.file_link_view(TextView::new(view), cx)),
            );
        }
        for message in messages {
            if !message.attachments.is_empty() {
                body = body.child(self.render_attachments(
                    &message.attachments,
                    false,
                    &format!("child-{}", message.id),
                    cx,
                ));
            }
        }
        super::transcript::transcript_row(body, row, state.list.clone())
    }
}

#[cfg(test)]
mod tests;

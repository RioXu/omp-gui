use super::*;
use crate::domain::activity::status_label;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_todos(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_3().px_2();
        if self.todos.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("OMP 尚未创建任务清单"),
            );
        }
        for phase in &self.todos {
            let tasks: Vec<_> = phase["tasks"].as_array().into_iter().flatten().collect();
            let done = tasks.iter().filter(|t| t["status"] == "completed").count();
            let mut section = div().flex().flex_col().gap_2().child(
                div()
                    .flex()
                    .gap_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(phase["name"].as_str().unwrap_or("任务").to_owned())
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{done}/{}", tasks.len())),
                    ),
            );
            for task in tasks {
                let status = task["status"].as_str().unwrap_or("pending");
                let color = match status {
                    "blocked" => cx.theme().danger,
                    "in_progress" => cx.theme().primary,
                    _ => cx.theme().muted_foreground,
                };
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .text_xs()
                            .text_color(color)
                            .child(status_label(status).to_owned()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(task["content"].as_str().unwrap_or_default().to_owned()),
                    );
                for key in ["blocker", "details"] {
                    if let Some(text) = task[key].as_str().filter(|s| !s.is_empty()) {
                        row = row.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(text.to_owned()),
                        );
                    }
                }
                for note in task["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    row = row.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(note.to_owned()),
                    );
                }
                section = section.child(row);
            }
            body = body.child(section);
        }
        body.into_any_element()
    }
    pub(in crate::desktop::workspace) fn render_subagents(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2().px_2();
        if let Some(id) = &self.collaboration.selected {
            let name = self
                .agents
                .iter()
                .find(|a| a["id"].as_str() == Some(id.as_str()))
                .and_then(|a| a["agent"].as_str())
                .unwrap_or(id);
            body = body
                .child(
                    Button::new("back-subagents")
                        .ghost()
                        .small()
                        .label("返回子智能体列表")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.collaboration = Default::default();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{name} · 会话历史")),
                );
            if let Some(error) = &self.collaboration.error {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }
            if self.collaboration.messages.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if self.collaboration.loading {
                            "正在读取子会话…"
                        } else {
                            "子会话暂未写入消息"
                        }),
                );
            }
            for message in &self.collaboration.messages {
                let key = format!("child-{id}-{}", message.id);
                let label = message
                    .label
                    .as_deref()
                    .unwrap_or(if message.role == "user" {
                        "任务输入"
                    } else {
                        name
                    });
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label.to_owned()),
                    );
                if !message.text.is_empty() {
                    row = row.child(self.file_link_view(
                        TextView::markdown(SharedString::from(key.clone()), message.text.clone()),
                        cx,
                    ));
                }
                for block in &message.process {
                    row = row.child(self.render_process_block(&key, block, cx));
                }
                body = body.child(row);
            }
        } else {
            if self.agents.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("本次会话没有子智能体"),
                );
            }
            for agent in &self.agents {
                let id = agent["id"].as_str().unwrap_or_default().to_owned();
                let name = agent["name"]
                    .as_str()
                    .or_else(|| agent["agent"].as_str())
                    .unwrap_or("子智能体");
                let mut card = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!(
                                "{name} · {}",
                                status_label(agent["status"].as_str().unwrap_or("pending"))
                            )),
                    );
                for key in ["description", "task", "assignment", "error", "output"] {
                    if let Some(text) = agent[key].as_str().filter(|s| !s.is_empty()) {
                        card = card.child(self.file_link_view(
                            TextView::markdown(
                                SharedString::from(format!("agent-{id}-{key}")),
                                text.to_owned(),
                            ),
                            cx,
                        ));
                    }
                }
                if let Some(tool) = agent["progress"]["currentTool"].as_str() {
                    card = card.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("正在执行 {tool}")),
                    );
                }
                card = card.child(
                    Button::new(SharedString::from(format!("agent-history-{id}")))
                        .ghost()
                        .small()
                        .label("查看会话")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_subagent(id.clone());
                            cx.notify();
                        })),
                );
                body = body.child(card);
            }
        }
        body.into_any_element()
    }
}

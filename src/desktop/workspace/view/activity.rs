use super::*;
use crate::domain::activity::status_label;
use gpui_kit::component::{
    marker::{Marker, MarkerContent, MarkerIcon},
    spinner::Spinner,
    tooltip::Tooltip,
};

impl Workspace {
    pub(in crate::desktop::workspace) fn render_todos(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_4().px_2();
        if self.runtime.todos.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("OMP 尚未创建任务清单"),
            );
        }
        for (phase_ix, phase) in self.runtime.todos.iter().enumerate() {
            let tasks: Vec<_> = phase["tasks"].as_array().into_iter().flatten().collect();
            let done = tasks.iter().filter(|t| t["status"] == "completed").count();
            let mut rows = div()
                .flex()
                .flex_col()
                .gap_3()
                .p_3()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md();
            for (task_ix, task) in tasks.iter().enumerate() {
                rows = rows.child(task_marker(
                    task,
                    format!("todo-{phase_ix}-{task_ix}").into(),
                    cx,
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(phase["name"].as_str().unwrap_or("任务").to_owned())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{done}/{}", tasks.len())),
                            ),
                    )
                    .when(!tasks.is_empty(), |section| section.child(rows)),
            );
        }
        body.into_any_element()
    }
    pub(in crate::desktop::workspace) fn render_subagents(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2().px_2();
        if let Some(id) = &self.runtime.collaboration.selected {
            let name = self
                .runtime
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
                            this.runtime.collaboration = Default::default();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{name} · 会话历史")),
                );
            if let Some(error) = &self.runtime.collaboration.error {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }
            if self.runtime.collaboration.messages.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if self.runtime.collaboration.loading {
                            "正在读取子会话…"
                        } else {
                            "子会话暂未写入消息"
                        }),
                );
            }
            for message in &self.runtime.collaboration.messages {
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
                        crate::desktop::markdown::view(
                            SharedString::from(key.clone()),
                            &message.text,
                        ),
                        cx,
                    ));
                }
                for block in &message.process {
                    row = row.child(self.render_process_block(&key, block, cx));
                }
                body = body.child(row);
            }
        } else {
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
                            crate::desktop::markdown::view(
                                SharedString::from(format!("agent-{id}-{key}")),
                                text,
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

fn task_marker(task: &Value, id: SharedString, cx: &App) -> Marker {
    let status = task["status"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("pending");
    let color = match status {
        "completed" => cx.theme().success,
        "started" | "running" | "in_progress" => cx.theme().primary,
        "blocked" | "failed" | "error" => cx.theme().danger,
        "pending" | "queued" | "idle" | "abandoned" | "aborted" | "cancelled" => {
            cx.theme().muted_foreground
        }
        _ => cx.theme().warning,
    };
    let icon = match status {
        "started" | "running" | "in_progress" => {
            Spinner::new().xsmall().color(color).into_any_element()
        }
        _ => Icon::new(match status {
            "completed" => assets::IconName::CircleCheck,
            "blocked" => assets::IconName::CircleAlert,
            "failed" | "error" => assets::IconName::CircleX,
            "abandoned" | "aborted" | "cancelled" => assets::IconName::CircleMinus,
            "pending" | "queued" | "idle" => assets::IconName::Circle,
            _ => assets::IconName::Info,
        })
        .size_4()
        .text_color(color)
        .into_any_element(),
    };
    let label = status_label(status).to_owned();
    let icon = div()
        .id("status")
        .role(Role::Image)
        .aria_label(label.clone())
        .tooltip(move |w, cx| Tooltip::new(label.clone()).build(w, cx))
        .child(icon);
    let mut content = MarkerContent::new()
        .flex_1()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().child(task["content"].as_str().unwrap_or_default().to_owned()));
    for text in ["blocker", "details"]
        .into_iter()
        .filter_map(|key| task[key].as_str())
        .filter(|s| !s.is_empty())
        .chain(
            task["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
    {
        content = content.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(text.to_owned()),
        );
    }
    Marker::new()
        .id(id)
        .items_start()
        .text_color(color)
        .icon(MarkerIcon::new().mt(px(2.)).child(icon))
        .content(content)
}

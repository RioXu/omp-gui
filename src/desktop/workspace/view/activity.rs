use super::*;
use crate::domain::activity::status_label;
use gpui_kit::component::{
    marker::{Marker, MarkerContent, MarkerIcon},
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
            icon_spinner().xsmall().color(color).into_any_element()
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

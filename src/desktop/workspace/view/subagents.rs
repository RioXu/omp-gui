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
        let agent = self.runtime.agents.iter().find(|agent| agent["id"] == id);
        let role = agent.map(subagent_role).unwrap_or("子智能体");
        let title = agent.map(subagent_title).unwrap_or("子会话");
        let mut body = div()
            .id(SharedString::from(format!("subagent-transcript-{id}")))
            .debug_selector({
                let id = id.to_owned();
                move || format!("subagent-transcript-{id}")
            })
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .px(relative(0.1))
            .py_4()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} · {}",
                        role,
                        status_label(
                            agent
                                .and_then(|a| a["status"].as_str())
                                .unwrap_or("pending")
                        )
                    )),
            );
        if let Some(agent) = agent {
            for key in ["task", "assignment", "error"] {
                if let Some(text) = agent[key]
                    .as_str()
                    .filter(|text| !text.is_empty() && *text != title)
                {
                    body = body.child(self.file_link_view(
                        crate::desktop::markdown::view(
                            SharedString::from(format!("child-{id}-{key}")),
                            text,
                        ),
                        cx,
                    ));
                }
            }
        }
        if let Some(error) = &self.runtime.collaboration.error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        if self.runtime.collaboration.messages.is_empty() {
            if let Some(output) = agent
                .and_then(|a| a["output"].as_str())
                .filter(|text| !text.is_empty())
            {
                body = body.child(self.file_link_view(
                    crate::desktop::markdown::view(
                        SharedString::from(format!("child-{id}-output")),
                        output,
                    ),
                    cx,
                ));
            } else {
                body = body.child(
                    div()
                        .py_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if self.runtime.collaboration.loading {
                            "正在读取子会话…"
                        } else {
                            "子会话暂未写入消息"
                        }),
                );
            }
        }
        for message in &self.runtime.collaboration.messages {
            let key = format!("child-{id}-{}", message.id);
            let label = message
                .label
                .as_deref()
                .unwrap_or(if message.role == "user" {
                    "任务输入"
                } else {
                    role
                });
            let mut row = div()
                .flex()
                .flex_col()
                .gap_2()
                .py_3()
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
                    crate::desktop::markdown::view(SharedString::from(key.clone()), &message.text),
                    cx,
                ));
            }
            for block in &message.process {
                row = row.child(self.render_process_block(&key, block, cx));
            }
            body = body.child(row);
        }
        body.into_any_element()
    }
}

#[cfg(test)]
mod tests;

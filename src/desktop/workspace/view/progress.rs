use super::*;
use gpui_kit::component::collapsible::Collapsible;

impl Workspace {
    pub(in crate::desktop::workspace) fn process_section(
        &self,
        key: String,
        label: String,
        open: bool,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = open || self.runtime.expanded_process.contains(&key);
        Collapsible::new()
            .open(expanded)
            .child(
                div().flex().items_center().child(
                    Button::new(SharedString::from(key.clone()))
                        .ghost()
                        .small()
                        .icon(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .label(label)
                        .text_color(cx.theme().muted_foreground)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if open {
                                return;
                            }
                            this.prepare_tool_diff(&key, cx);
                            if !this.runtime.expanded_process.remove(&key) {
                                this.runtime.expanded_process.insert(key.clone());
                            }
                            this.invalidate_list();
                            cx.notify();
                        })),
                ),
            )
            .content(div().pl_3().py_2().child(content))
            .into_any_element()
    }
    pub(in crate::desktop::workspace) fn render_process_block(
        &self,
        message_id: &str,
        block: &ProcessBlock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        if block.kind == ProcessKind::Thinking {
            return self.render_thinking(message_id, block, cx);
        }
        if block.is_read() {
            let selector = format!("process-read-{message_id}-{}", block.id);
            let executing = self.runtime.active_tools.contains_key(&block.id);
            return div()
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(if executing {
                    icon_spinner()
                        .xsmall()
                        .color(theme.primary)
                        .into_any_element()
                } else {
                    Icon::new(IconName::File).size_3().into_any_element()
                })
                .child(div().flex_1().min_w_0().truncate().child(format!(
                    "{} {}",
                    if executing { "正在读取" } else { "读取" },
                    block.path()
                )))
                .into_any_element();
        }
        let key = format!("process-{message_id}-{}", block.id);
        let label = if block.is_edit() {
            format!("编辑 {}", block.path())
        } else if !block.computer_operations.is_empty() {
            "电脑操作".into()
        } else {
            block.label.clone()
        };
        let expanded = block.streaming || self.runtime.expanded_process.contains(&key);
        let mut detail = div().flex().flex_col().gap_2().min_w_0();
        if expanded {
            for operation in &block.computer_operations {
                detail = detail.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(operation.clone()),
                );
            }
            if block.is_edit() {
                if let Some(diff) = self.runtime.tool_diffs.get(&key) {
                    detail = detail.child(
                        div()
                            .h_64()
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded_md()
                            .overflow_hidden()
                            .child(Diff::new(diff)),
                    );
                } else {
                    detail = detail.child(
                        self.process_text(
                            format!("edit-{key}"),
                            block
                                .patch
                                .as_deref()
                                .or(block.result.as_deref())
                                .unwrap_or("等待编辑结果…"),
                            "diff",
                            false,
                            window,
                            cx,
                        ),
                    );
                }
            } else {
                let command = if block.text.len() <= 8192 {
                    block.command()
                } else {
                    String::new()
                };
                if !command.is_empty() {
                    detail = detail.child(self.process_text(
                        format!("command-{key}"),
                        &command,
                        "sh",
                        false,
                        window,
                        cx,
                    ));
                } else if !block.text.trim().is_empty() && block.text.trim() != "{}" {
                    detail = detail.child(self.process_text(
                        format!("args-{key}"),
                        &block.text,
                        "json",
                        false,
                        window,
                        cx,
                    ));
                }
                if let Some(result) = &block.result {
                    detail = detail.child(self.process_text(
                        format!("output-{key}"),
                        result,
                        "text",
                        true,
                        window,
                        cx,
                    ));
                }
            }
        }
        self.process_section(key, label, block.streaming, detail, cx)
    }
}

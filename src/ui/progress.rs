use super::*;
use gpui_kit::component::collapsible::Collapsible;

impl Workspace {
    pub(super) fn process_section(
        &self,
        key: String,
        label: String,
        open: bool,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = open || self.expanded_process.contains(&key);
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
                            if !this.expanded_process.remove(&key) {
                                this.expanded_process.insert(key.clone());
                            }
                            this.invalidate_list();
                            cx.notify();
                        })),
                ),
            )
            .content(div().pl_3().py_2().child(content))
            .into_any_element()
    }
    pub(super) fn render_process_block(
        &self,
        message_id: &str,
        block: &ProcessBlock,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        if block.is_read() {
            let executing = self.active_tools.contains_key(&block.id);
            return div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(if executing {
                    gpui_kit::component::spinner::Spinner::new()
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
        let label = if block.kind == ProcessKind::Thinking {
            if block.streaming {
                "正在思考…".into()
            } else {
                "思考过程".into()
            }
        } else if block.is_edit() {
            format!("编辑 {}", block.path())
        } else {
            block.label.clone()
        };
        let expanded = block.streaming || self.expanded_process.contains(&key);
        let mut detail = div().flex().flex_col().gap_2().min_w_0();
        if expanded {
            if block.kind == ProcessKind::Thinking {
                detail = detail.child(TextView::markdown(
                    SharedString::from(format!("thinking-{key}")),
                    block.text.clone(),
                ));
            } else if block.is_edit() {
                if let Some(diff) = self.tool_diffs.get(&key) {
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
                    detail = detail.child(TextView::markdown(
                        SharedString::from(format!("edit-{key}")),
                        fenced(
                            block
                                .patch
                                .as_deref()
                                .or(block.result.as_deref())
                                .unwrap_or("等待编辑结果…"),
                            "diff",
                        ),
                    ));
                }
            } else {
                let command = block.command();
                if !command.is_empty() {
                    detail = detail.child(TextView::markdown(
                        SharedString::from(format!("command-{key}")),
                        fenced(&command, "sh"),
                    ));
                }
                if let Some(result) = &block.result {
                    detail = detail.child(TextView::markdown(
                        SharedString::from(format!("output-{key}")),
                        fenced(result, "text"),
                    ));
                }
            }
        }
        self.process_section(key, label, block.streaming, detail, cx)
    }
}

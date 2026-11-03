use super::*;

const PREVIEW_HEIGHT: f32 = 105.;

impl Workspace {
    pub(super) fn render_thinking(
        &self,
        message_id: &str,
        block: &ProcessBlock,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("process-{message_id}-{}", block.id);
        let text_id = SharedString::from(format!("thinking-text-{message_id}-{}", block.id));
        let expanded = self.runtime.expanded_process.contains(&key);
        let preview = block.streaming || self.runtime.thinking_previews.contains(&key);
        let message_id = message_id.to_owned();
        let title = if block.streaming {
            "正在思考…"
        } else {
            "思考过程"
        };
        let action = if expanded { "收起" } else { "展开" };
        let mut card = div()
            .id(SharedString::from(format!("thinking-{key}")))
            .debug_selector(|| "thinking-card".into())
            .flex()
            .flex_col()
            .gap_1()
            .min_w_0()
            .when(expanded || preview, |card| {
                card.border_1()
                    .border_color(crate::desktop::theme::thinking_color(cx))
                    .rounded_md()
                    .p_2()
            })
            .child(
                div().flex().items_start().child(
                    Button::new(SharedString::from(key.clone()))
                        .debug_selector(|| "thinking-toggle".into())
                        .ghost()
                        .small()
                        .flex_shrink_0()
                        .icon(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .label(title)
                        .accessibility_label(format!("{action}思考过程"))
                        .text_color(cx.theme().muted_foreground)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.runtime.thinking_previews.remove(&key);
                            if !this.runtime.expanded_process.remove(&key) {
                                this.runtime.expanded_process.insert(key.clone());
                            }
                            let at_tail = this.runtime.list.logical_scroll_top().item_ix
                                >= this.runtime.list.item_count();
                            this.invalidate_list();
                            // This is an explicit user expansion, not a stream update.
                            if at_tail {
                                let row = this.row_for_message(this.message_index(&message_id));
                                this.runtime.list.scroll_to(ListOffset {
                                    item_ix: row,
                                    offset_in_item: px(0.),
                                });
                            }
                            cx.notify();
                        })),
                ),
            );
        if expanded || preview {
            // Preview and full mode share Markdown parsing and the same width.
            let content = div()
                .debug_selector(|| "thinking-preview-text".into())
                .w_full()
                .flex_shrink_0()
                .child(
                    crate::desktop::markdown::view(text_id, &block.text)
                        .text_sm()
                        .text_color(crate::desktop::theme::thinking_color(cx)),
                );
            card = card.child(div().pl_3().w_full().child(if expanded {
                div()
                    .debug_selector(|| "thinking-full".into())
                    .w_full()
                    .child(content)
                    .into_any_element()
            } else {
                div()
                    .debug_selector(|| "thinking-preview".into())
                    .w_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .max_h(px(PREVIEW_HEIGHT))
                    .overflow_hidden()
                    .child(content)
                    .into_any_element()
            }));
        }
        card.into_any_element()
    }
}

#[cfg(test)]
mod tests;

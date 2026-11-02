use super::*;
mod lsp;

impl Workspace {
    pub(super) fn render_notice(&self, message: &Message, cx: &mut Context<Self>) -> AnyElement {
        let key = format!("notice-{}", message.id);
        let message_id = message.id.clone();
        let expanded = self.runtime.expanded_process.contains(&key);
        let title = match message.label.as_deref() {
            Some("lsp-late-diagnostic") => "LSP 延迟诊断",
            Some("lsp-diagnostic" | "lsp-diagnostics") => "LSP 诊断",
            Some("async-result") => "后台任务结果",
            Some(title) => title,
            None => "会话消息",
        };
        let action = if expanded { "收起" } else { "展开" };
        let mut card = div()
            .debug_selector(|| "conversation-notice-body".into())
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .mx_auto()
            .my_3()
            .p_3()
            .border_l_2()
            .border_color(cx.theme().primary)
            .bg(cx.theme().muted)
            .rounded_md()
            .child(
                Button::new(SharedString::from(key.clone()))
                    .debug_selector(|| "notice-toggle".into())
                    .ghost()
                    .small()
                    .w_full()
                    .min_w_0()
                    .accessibility_label(format!("{title} {action}"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .w_full()
                            .min_w_0()
                            .child(
                                Icon::new(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .size_3(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .child(title.to_owned()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(action),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.runtime.expanded_process.remove(&key) {
                            this.runtime.expanded_process.insert(key.clone());
                        }
                        let at_tail_boundary = this.runtime.list.logical_scroll_top().item_ix
                            >= this.runtime.list.item_count();
                        this.invalidate_list();
                        if at_tail_boundary {
                            let row = this.row_for_message(this.message_index(&message_id));
                            this.runtime.list.scroll_to(ListOffset {
                                item_ix: row,
                                offset_in_item: px(0.),
                            });
                        }
                        cx.notify();
                    })),
            );
        if let Some(view) = self.runtime.texts.get(&message.id) {
            card = card.child(
                div()
                    .debug_selector(|| "notice-content".into())
                    .w_full()
                    .child(
                        self.message_file_link_view(
                            TextView::new(&view.state).when(!expanded, |view| view.max_lines(3)),
                            &message.id,
                            cx,
                        )
                        .when(
                            omp_core::domain::notice::lsp::is_lsp(message),
                            |view| {
                                view.plugin(lsp::LspBlocks {
                                    workspace: cx.entity().downgrade(),
                                    session: self
                                        .session()
                                        .map(|s| s.id.clone())
                                        .unwrap_or_default(),
                                })
                            },
                        ),
                    ),
            );
        }
        if let Some(session) = self.session() {
            for id in &message.comments {
                if let Some(comment) = session.comments.iter().find(|c| &c.id == id) {
                    card = card.child(self.render_comment(comment, cx));
                }
            }
        }
        card.into_any_element()
    }
}

#[cfg(test)]
mod tests;

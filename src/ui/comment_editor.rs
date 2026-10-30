use super::*;

impl Workspace {
    pub fn render_comment_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(anchor) = &self.comment_editor else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .w_80()
            .bg(cx.theme().popover.opacity(1.0))
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .shadow_lg()
            .child(div().text_sm().child(anchor.label()))
            .child(
                div()
                    .max_h_32()
                    .id("comment-quote")
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(anchor.quote.clone()),
            )
            .child(Textarea::new(&self.comment_input).aria_label("评论内容"))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-comment")
                            .ghost()
                            .small()
                            .label("取消")
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.comment_editor = None;
                                this.selected = None;
                                this.input.focus_handle(cx).focus(w, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("save-comment")
                            .primary()
                            .small()
                            .label("加入待发送")
                            .on_click(cx.listener(|this, _, w, cx| this.save_comment(w, cx))),
                    ),
            )
            .into_any_element()
    }
}

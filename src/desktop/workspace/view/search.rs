use super::*;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_search(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.search_state.hits.len();
        div()
            .key_context("SessionSearch")
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .child(
                div().flex_1().child(
                    Input::new(&self.search_state.input)
                        .small()
                        .aria_label("查找当前标签页"),
                ),
            )
            .child(div().text_xs().child(if count == 0 {
                "0 项匹配".into()
            } else {
                format!("{} / {count}", self.search_state.index + 1)
            }))
            .child(
                Button::new("search-prev")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronUp)
                    .accessibility_label("上一项")
                    .tooltip("上一项 · Shift+Enter")
                    .disabled(count == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.move_search(true, cx))),
            )
            .child(
                Button::new("search-next")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronDown)
                    .accessibility_label("下一项")
                    .tooltip("下一项 · Enter")
                    .disabled(count == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.move_search(false, cx))),
            )
            .child(
                Button::new("search-close")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .tooltip("Esc · 关闭查找")
                    .on_click(cx.listener(|this, _, w, cx| this.close_search(w, cx))),
            )
            .into_any_element()
    }

    pub(in crate::desktop::workspace) fn render_completions(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self
            .runtime
            .composer_state
            .input
            .focus_handle(cx)
            .is_focused(window)
        {
            return div().into_any_element();
        }
        let items = self.completions(cx);
        if items.is_empty() {
            return div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .children(self.completion_hint(cx))
                .into_any_element();
        }
        // Keep the active item visible without moving focus out of the textarea.
        let selected = self
            .runtime
            .composer_state
            .selected_completion
            .min(items.len() - 1);
        let start = selected.saturating_sub(5);
        let mut menu = div()
            .flex()
            .flex_col()
            .p_2()
            .rounded_lg()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("↑↓ 选择 · Tab / Enter 确认 · Esc 关闭"),
            );
        for (ix, item) in items.into_iter().enumerate().skip(start).take(6) {
            menu = menu.child(
                ListItem::new(SharedString::from(format!("completion-{ix}")))
                    .selected(ix == selected)
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        // The menu belongs to the composer. Retain its focus
                        // until MouseUp dispatches the item's click handler.
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(div().truncate().child(item.label))
                            .child(
                                div()
                                    .text_xs()
                                    .truncate()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(item.description),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.accept_completion(ix, w, cx))),
            );
        }
        menu.into_any_element()
    }
}

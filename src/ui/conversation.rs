use super::*;

impl Workspace {
    pub(super) fn render_comment(&self, c: &Comment, cx: &mut Context<Self>) -> AnyElement {
        let cid = c.id.clone();
        let a = c.anchor.clone();
        let reply = cid.clone();
        let resolve = cid.clone();
        let theme = cx.theme().clone();
        let mut actions = div().flex().gap_2().flex_wrap().child(
            Button::new(SharedString::from(format!("anchor-{cid}")))
                .ghost()
                .xsmall()
                .label(if matches!(a.source, Source::File { .. }) {
                    "查看当时变更"
                } else {
                    "定位原文"
                })
                .on_click(cx.listener(move |this, _, _, cx| this.open_anchor(a.clone(), cx))),
        );
        if let Source::File { path, .. } = &c.anchor.source {
            let path = path.clone();
            actions = actions.child(
                Button::new(SharedString::from(format!("current-{cid}")))
                    .ghost()
                    .xsmall()
                    .label("打开当前代码")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_panel("变更");
                        this.viewed_path = Some(path.clone());
                        this.open_current();
                        cx.notify();
                    })),
            );
        }
        let info = if c.resolved {
            "你已标记解决"
        } else if c.reply_ids.is_empty() {
            "待回应"
        } else {
            "已有跟进 · 待确认"
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_2()
            .mb_3()
            .p_3()
            .bg(theme.muted)
            .rounded_lg()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} · {}", c.anchor.label(), &cid[..8])),
            )
            .child(TextView::markdown(
                SharedString::from(format!("quote-{cid}")),
                format!("> {}", c.anchor.quote.replace('\n', "\n> ")),
            ))
            .child(c.text.clone())
            .child(actions)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(info)
                    .child(
                        Button::new(SharedString::from(format!("follow-{cid}")))
                            .ghost()
                            .xsmall()
                            .label("继续跟进")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.follow_up = Some(reply.clone());
                                this.input.focus_handle(cx).focus(w, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("resolve-{cid}")))
                            .ghost()
                            .xsmall()
                            .label(if c.resolved {
                                "重新打开"
                            } else {
                                "标记已解决"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.session_mut()
                                    && let Some(c) = s.comments.iter_mut().find(|c| c.id == resolve)
                                {
                                    c.resolved = !c.resolved;
                                }
                                this.persist();
                                this.invalidate_list();
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

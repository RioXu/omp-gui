use super::*;

impl Workspace {
    pub(in crate::desktop::workspace) fn render_requests(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(r) = self.requests.front() else {
            return div().into_any_element();
        };
        let method = r["method"].as_str().unwrap_or("");
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .mx_6()
            .bg(cx.theme().muted)
            .rounded_lg()
            .child(
                r["title"]
                    .as_str()
                    .unwrap_or("OMP 需要你的回应")
                    .to_string(),
            );
        if let Some(message) = r["message"].as_str() {
            body = body.child(message.to_string());
        }
        if method == "select" {
            for (ix, v) in r["options"].as_array().into_iter().flatten().enumerate() {
                let text = v.as_str().unwrap_or("").to_string();
                let value = text.clone();
                body = body.child(
                    Button::new(SharedString::from(format!("request-option-{ix}")))
                        .small()
                        .label(text)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.answer_request(Some(value.clone()), None);
                            cx.notify();
                        })),
                );
            }
        } else if method == "confirm" {
            body = body.child(
                Button::new("approve")
                    .small()
                    .label("允许")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.answer_request(None, Some(true));
                        cx.notify();
                    })),
            );
        } else {
            body = body.child(Input::new(&self.answer_input)).child(
                Button::new("answer")
                    .small()
                    .label("提交回答")
                    .on_click(cx.listener(|this, _, w, cx| {
                        let v = this.answer_input.read(cx).value().to_string();
                        this.answer_request(Some(v), None);
                        this.answer_input.update(cx, |i, cx| i.set_value("", w, cx));
                        cx.notify();
                    })),
            );
        }
        body.child(
            Button::new("deny")
                .ghost()
                .small()
                .label("取消 / 拒绝")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.answer_request(None, None);
                    cx.notify();
                })),
        )
        .into_any_element()
    }
}

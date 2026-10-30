use super::*;

impl RegistryView {
    pub(super) fn model_form(&self, cx: &mut Context<Self>) -> AnyElement {
        v_form()
            .layout(Axis::Horizontal)
            .child(
                field().label("模型 ID").child(
                    Input::new(&self.fields["model_id"]).disabled(self.model_edit.is_some()),
                ),
            )
            .child(
                field()
                    .label("显示名称")
                    .child(Input::new(&self.fields["model_name"])),
            )
            .child(
                field()
                    .label("上下文长度 · tokens")
                    .child(Input::new(&self.fields["context"])),
            )
            .child(
                field()
                    .label("最大输出 · tokens")
                    .child(Input::new(&self.fields["tokens"])),
            )
            .child(
                field().label("推理模型").child(
                    div().w_full().flex().justify_end().child(
                        Switch::new("model-reasoning")
                            .checked(self.reasoning)
                            .on_click(cx.listener(|this, v: &bool, _, cx| {
                                this.reasoning = *v;
                                cx.notify();
                            })),
                    ),
                ),
            )
            .child(
                field().label("支持图片输入").child(
                    div().w_full().flex().justify_end().child(
                        Switch::new("model-vision")
                            .checked(self.vision)
                            .on_click(cx.listener(|this, v: &bool, _, cx| {
                                this.vision = *v;
                                cx.notify();
                            })),
                    ),
                ),
            )
            .child(
                field().child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("apply-model")
                                .primary()
                                .label("保留模型配置")
                                .on_click(cx.listener(|this, _, _, cx| this.save_model(cx))),
                        )
                        .child(Button::new("cancel-model").ghost().label("取消").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.model_form = false;
                                cx.notify();
                            }),
                        )),
                ),
            )
            .into_any_element()
    }
}

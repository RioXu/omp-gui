use super::*;

impl Workspace {
    pub(super) fn render_working_intent(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(intent) = self.runtime.working_intent.as_ref().filter(|_| {
            self.runtime.running && !self.runtime.stopping && !self.runtime.reconnecting
        }) else {
            return div().into_any_element();
        };
        let tooltip = intent.clone();
        div()
            .id("working-intent")
            .debug_selector(|| "working-intent".into())
            .role(Role::Status)
            .aria_label(intent.clone())
            .flex()
            .items_center()
            .gap_2()
            .h_6()
            .w_full()
            .min_w_0()
            .text_base()
            .text_color(cx.theme().muted_foreground)
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(icon_spinner().small().color(cx.theme().primary))
            .child(div().flex_1().min_w_0().truncate().child(intent.clone()))
            .into_any_element()
    }
}

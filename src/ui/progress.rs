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
                        .on_click(cx.listener(move |this, _, _, cx| {
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
}

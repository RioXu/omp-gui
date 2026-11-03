use super::*;

const TASKS_KEY: &str = "composer-todos";

impl Workspace {
    pub(super) fn render_task_toggle(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let open = self.runtime.expanded_process.contains(TASKS_KEY);
        let tasks = self
            .runtime
            .todos
            .iter()
            .flat_map(|phase| phase["tasks"].as_array().into_iter().flatten());
        let (done, total) = tasks.fold((0, 0), |(done, total), task| {
            (done + usize::from(task["status"] == "completed"), total + 1)
        });
        let weak = cx.entity().downgrade();
        let contents = weak.clone();
        let session = self.session().map(|s| s.id.clone()).unwrap_or_default();
        let popup_id = SharedString::from(format!("composer-tasks-{session}"));
        let width = f32::from(window.viewport_size().width);
        let sidebar = if width > 1000. || !self.panel_open {
            self.db.sidebar_width
        } else {
            0.
        };
        let panel = if self.panel_open {
            self.db.panel_width
        } else {
            0.
        };
        let popup_width = (width - sidebar - panel - 32.).clamp(200., 360.);
        div()
            .flex_shrink_0()
            .child(
                Popover::new(popup_id)
                    .anchor(gpui_kit::Anchor::BottomRight)
                    .open(open)
                    .p_0()
                    .w(px(popup_width))
                    .rounded_xl()
                    .trigger(
                        Button::new("composer-tasks-toggle")
                            .debug_selector(|| "composer-tasks-toggle".into())
                            .small()
                            .rounded_full()
                            .selected(open)
                            .icon(if open {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .label(if total > 0 {
                                format!("任务 {done}/{total}")
                            } else {
                                "任务".into()
                            })
                            .accessibility_label(if open {
                                "收起任务状态"
                            } else {
                                "展开任务状态"
                            }),
                    )
                    .on_open_change(move |open, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if !this.session().is_some_and(|s| s.id == session) {
                                return;
                            }
                            if *open {
                                this.runtime.expanded_process.insert(TASKS_KEY.into());
                            } else {
                                this.runtime.expanded_process.remove(TASKS_KEY);
                            }
                            cx.notify();
                        });
                    })
                    .content(move |_, window, cx| {
                        contents
                            .update(cx, |this, cx| this.render_task_popup(window, cx))
                            .unwrap_or_else(|_| div().into_any_element())
                    }),
            )
            .into_any_element()
    }

    fn render_task_popup(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let height = px((f32::from(window.viewport_size().height) * 0.4).clamp(120., 400.));
        div()
            .debug_selector(|| "composer-task-panel".into())
            .w_full()
            .overflow_hidden()
            .rounded_xl()
            .child(
                div()
                    .id("composer-task-scroll")
                    .w_full()
                    .max_h(height)
                    .py_3()
                    .child(self.render_todos(cx))
                    .overflow_y_scrollbar(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;

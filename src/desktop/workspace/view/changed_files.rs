//! Flat changed-file rows, sharing status colors with the project file tree.
use super::*;
use crate::infrastructure::filesystem::changes::ChangeKind;

pub(super) fn change_color(kind: ChangeKind, cx: &App) -> Hsla {
    match kind {
        ChangeKind::Added => cx.theme().success,
        ChangeKind::Deleted => cx.theme().danger,
        ChangeKind::Conflicted => cx.theme().warning,
        ChangeKind::Modified | ChangeKind::Renamed => rgb(if cx.theme().is_dark() {
            0xe6d89c
        } else {
            0x8a7028
        })
        .into(),
    }
}

impl Workspace {
    pub(super) fn render_changes(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.document_state.changes.is_empty() {
            return div()
                .p_3()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("暂无文件变更")
                .into_any_element();
        }
        let changes = self.document_state.changes.clone();
        uniform_list(
            "changed-files",
            changes.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|ix| {
                        let change = &changes[ix];
                        let path = change.path.clone();
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let parent = path
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        ListItem::new(SharedString::from(format!("change-{}", path.display())))
                            .accessibility_label(path.to_string_lossy().into_owned())
                            .h_7()
                            .px_2()
                            .py_0()
                            .rounded_none()
                            .selected(this.document_state.path.as_ref() == Some(&path))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(
                                        Icon::new(IconName::File)
                                            .size_4()
                                            .flex_shrink_0()
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .min_w_0()
                                            .max_w(relative(0.7))
                                            .truncate()
                                            .text_color(change_color(change.kind, cx))
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .min_w_0()
                                            .flex_1()
                                            .truncate()
                                            .child(parent),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_document(path.clone(), true);
                                cx.notify();
                            }))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .flex_1()
        .min_h_0()
        .into_any_element()
    }
}

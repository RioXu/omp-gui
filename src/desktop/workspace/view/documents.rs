use super::*;
use gpui_kit::component::{
    input::Editor,
    tab::{Tab, TabBar},
};

impl Workspace {
    pub(in crate::desktop::workspace) fn render_document_tabs(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tabs = TabBar::new("document-tabs")
            .large()
            .h(px(36.))
            .menu(false)
            .max_width(px(220.))
            .prefix(
                div().id("pinned-session-tab").flex_shrink_0().child(
                    Tab::new()
                        .large()
                        .label("会话")
                        .suffix(div().h(px(36.)).w_px().bg(cx.theme().border))
                        .selected(self.document_state.active.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.activate_document(None);
                            cx.notify();
                        })),
                ),
            );
        if let Some(ix) = self
            .document_state
            .active
            .as_ref()
            .and_then(|key| self.document_state.tabs.iter().position(|t| &t.key == key))
        {
            tabs = tabs.selected_index(ix);
        }
        for tab in &self.document_state.tabs {
            let key = tab.key.clone();
            let close_key = key.clone();
            let name = tab
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let label = if tab.historical {
                format!("{name} · 历史")
            } else {
                name
            };
            tabs = tabs.child(
                Tab::new()
                    .aria_label(label.clone())
                    .child(
                        div()
                            .truncate()
                            .when(tab.changes, |d| {
                                d.text_color(rgb(if cx.theme().is_dark() {
                                    0xe6d89c
                                } else {
                                    0x8a7028
                                }))
                            })
                            .child(label),
                    )
                    .selected(Some(&tab.key) == self.document_state.active.as_ref())
                    .when(tab.changes, |t| {
                        t.prefix(
                            div().pl_3().child(
                                Icon::new(assets::IconName::GitBranch)
                                    .size_4()
                                    .text_color(cx.theme().muted_foreground),
                            ),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.activate_document(Some(key.clone()));
                        cx.notify();
                    }))
                    .suffix(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .h(px(36.))
                            .pr_2()
                            .child(
                                Button::new(SharedString::from(format!("close-{}", tab.key)))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Close)
                                    .accessibility_label("关闭标签页")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_document(&close_key);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right_0()
                                    .top_0()
                                    .bottom_0()
                                    .w_px()
                                    .bg(cx.theme().border),
                            ),
                    ),
            );
        }
        let items = self
            .document_state
            .tabs
            .iter()
            .map(|t| (t.key.clone(), t.path.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        let weak = cx.entity().downgrade();
        if !items.is_empty() {
            tabs = tabs.suffix(
                Button::new("document-overflow")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronDown)
                    .accessibility_label("打开的标签页")
                    .dropdown_menu(move |mut menu, _, _| {
                        for (key, label) in &items {
                            let key = key.clone();
                            let weak = weak.clone();
                            menu = menu.item(PopupMenuItem::new(label.clone()).on_click(
                                move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.activate_document(Some(key.clone()));
                                        cx.notify();
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            );
        }
        tabs.flex_1().min_w_0().into_any_element()
    }

    pub(in crate::desktop::workspace) fn render_document(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref())
        else {
            return div().into_any_element();
        };
        let mut view = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(theme.background)
            .child(
                div()
                    .h_9()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_4()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.5))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(tab.path.to_string_lossy().replace('\\', " / "))
                    .child(div().flex_1())
                    .when(tab.historical, |d| {
                        d.child("评论时版本").child(
                            Button::new("open-current-tab")
                                .ghost()
                                .xsmall()
                                .label("打开当前文件")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_current();
                                    cx.notify();
                                })),
                        )
                    }),
            );
        if let Some(error) = &tab.error {
            view = view.child(
                div()
                    .px_4()
                    .py_2()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        if let Some(quote) = &tab.quote {
            view = view.child(
                div()
                    .mx_4()
                    .my_2()
                    .p_2()
                    .border_l_2()
                    .border_color(theme.primary)
                    .text_sm()
                    .child(quote.clone()),
            );
        }
        view = if let Some(diff) = &tab.diff {
            view.child(div().flex_1().min_h_0().child(Diff::new(diff).size_full()))
        } else if let Some(editor) = &tab.editor {
            view.child(
                div().flex_1().min_h_0().child(
                    Editor::new(editor)
                        .readonly(true)
                        .appearance(false)
                        .bordered(false)
                        .size_full(),
                ),
            )
        } else if let Some(entry) = &tab.entry {
            view.child(
                div()
                    .id("historical-document")
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .child(TextView::new(&entry.state))
                    .overflow_y_scrollbar(),
            )
        } else {
            view.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(if tab.error.is_some() {
                        "无法预览此文件"
                    } else {
                        "正在打开…"
                    }),
            )
        };
        view.child(
            div()
                .h_7()
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_4()
                .gap_3()
                .border_t_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(div().flex_1())
                .child(if tab.historical {
                    "历史引用"
                } else {
                    "只读"
                })
                .child(crate::desktop::workspace::documents::language_for_path(
                    &tab.path,
                ))
                .child("UTF-8"),
        )
        .into_any_element()
    }
}

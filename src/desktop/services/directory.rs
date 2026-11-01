use super::*;

impl RegistryView {
    pub(super) fn directory(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut body =
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child("当前环境 OMP 可用服务与自定义配置"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(
                            Select::new(&self.provider_picker).placeholder("选择 OMP 服务…"),
                        ))
                        .child(
                            Button::new("add-native-provider")
                                .label("配置所选服务")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, w, cx| {
                                    if let Some(id) =
                                        this.provider_picker.read(cx).selected_value().cloned()
                                    {
                                        this.edit_provider(Some(id), w, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("custom-provider")
                                .label("自定义")
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, w, cx| this.edit_provider(None, w, cx)),
                                ),
                        ),
                );
        let mut registered = self.services.registered_providers();
        registered.extend(self.providers.values.keys().cloned());
        if registered.is_empty() {
            body = body.child(if self.catalog_loading {
                "正在读取当前环境 OMP 服务…"
            } else {
                "尚未发现可用服务；可选择服务配置连接。"
            });
        }
        for id in registered {
            let native = self.services.models.get(&id);
            let available = native
                .into_iter()
                .flatten()
                .filter(|m| self.services.available.contains(&registry::selector(m)))
                .count();
            let custom = self.providers.values.contains_key(&id);
            let action = id.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .child(
                        div().flex_1().child(id.clone()).child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{available} 个可用模型{}",
                                    if custom {
                                        " · 自定义配置"
                                    } else {
                                        " · OMP 原生服务"
                                    }
                                )),
                        ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("manage-{id}")))
                            .small()
                            .label("管理")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.edit_provider(Some(action.clone()), w, cx)
                            })),
                    ),
            );
        }
        body.child(div().text_xs().text_color(theme.muted_foreground)
            .child("目录基于当前环境的 OMP 模型数据。需要浏览器登录的账号请在对应环境的 OMP 终端完成登录；未覆盖的配置仍可在终端管理。"))
            .into_any_element()
    }
    pub(super) fn search_directory(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2().child("OMP 搜索服务目录");
        let available = self
            .services
            .search_options()
            .into_iter()
            .filter(|s| self.services.available.contains(s))
            .collect::<Vec<_>>();
        if available.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .child("尚未发现可用搜索服务，或目录正在加载。"),
            );
        }
        // Large catalogs remain virtualized; only visible service rows are built.
        body = body.child(
            div()
                .relative()
                .h_40()
                .flex_shrink_0()
                .child(
                    uniform_list(
                        "available-search-services",
                        available.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            let selected = this.search_service.read(cx).selected_value().cloned();
                            range
                                .map(|ix| {
                                    let id = available[ix].clone();
                                    let value = id.clone();
                                    div()
                                        .h_8()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div().flex_1().text_sm().truncate().child(id.clone()),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "select-search-{ix}"
                                            )))
                                            .ghost()
                                            .xsmall()
                                            .label(if selected.as_ref() == Some(&id) {
                                                "首选"
                                            } else {
                                                "选择"
                                            })
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                this.search_service.update(cx, |s, cx| {
                                                    s.set_selected_value(&value, w, cx)
                                                });
                                                cx.notify();
                                            })),
                                        )
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.search_scroll)
                    .h_full(),
                )
                .vertical_scrollbar(&self.search_scroll),
        );
        body.into_any_element()
    }
}

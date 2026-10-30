use super::*;

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(registry) = &self.registry {
            return div().size_full().child(registry.clone());
        }
        let theme = cx.theme().clone();
        let mut nav = div()
            .w_40()
            .h_full()
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .border_r_1()
            .border_color(theme.border);
        for category in CATEGORIES {
            nav = nav.child(
                SidebarMenu::new()
                    .child(
                        SidebarMenuItem::new(category)
                            .active(self.category == category)
                            .disable(self.edit.is_some())
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.category = category;
                                this.prepare_controls(w, cx);
                                cx.notify();
                            })),
                    )
                    .render(category, window, cx),
            );
        }
        let mut content = div().flex().flex_col().flex_1().min_w_0().h_full();
        content = content.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .p_4()
                .border_b_1()
                .border_color(theme.border)
                .child(div().text_lg().flex_1().child(self.category))
                .child(
                    Button::new("scope-global")
                        .small()
                        .label("全局")
                        .selected(!self.project)
                        .when(!self.project, |b| b.primary())
                        .disabled(self.edit.is_some())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.project = false;
                            this.prepare_controls(w, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("scope-project")
                        .small()
                        .label(
                            self.project_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|p| p.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "项目".into()),
                        )
                        .tooltip(
                            self.project_path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "请先选择项目会话".into()),
                        )
                        .selected(self.project)
                        .when(self.project, |b| b.primary())
                        .disabled(self.edit.is_some() || self.project_path.is_none())
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.project = true;
                            this.prepare_controls(w, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("reload-settings")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .tooltip("重新读取 OMP 配置")
                        .disabled(self.edit.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reload();
                            cx.notify();
                        })),
                ),
        );
        let mut body = div()
            .id("settings-body")
            .flex_1()
            .min_h_0()
            .when(self.edit.is_some(), |d| d.overflow_y_scroll())
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if let Some(edit) = &self.edit {
            let (label, description) = match edit {
                Edit::Key { name, kind } => (
                    name.clone(),
                    format!("类型：{kind}。对象和数组使用 JSON；重置会移除当前范围的值。"),
                ),
                Edit::File { path, .. } => (
                    path.display().to_string(),
                    "编辑 OMP 原生配置。保存前校验格式并保留备份；注册项遵循本机 OMP 版本。".into(),
                ),
            };
            body = body
                .child(div().text_sm().font_family("Cascadia Code").child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(description),
                );
            if matches!(edit,Edit::Key{kind,..} if ["string","enum","number"].contains(&kind.as_str()))
            {
                body = body.child(
                    div()
                        .max_w(rems(34.))
                        .child(Input::new(&self.scalar).aria_label("设置值")),
                );
            } else if !matches!(edit,Edit::Key{kind,..} if kind=="boolean") {
                body = body.child(Textarea::new(&self.editor).aria_label("设置值"));
            }
            if matches!(edit,Edit::Key{kind,..}if kind=="boolean") {
                let enabled = self.scalar.read(cx).value().trim() == "true";
                body = body.child(Switch::new("setting-toggle").checked(enabled).on_click(
                    cx.listener(|this, v: &bool, w, cx| {
                        this.scalar
                            .update(cx, |i, cx| i.set_value(v.to_string(), w, cx));
                        cx.notify();
                    }),
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("save-native")
                            .primary()
                            .label("保存")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save(false, cx))),
                    )
                    .when(matches!(edit, Edit::Key { .. }), |d| {
                        d.child(
                            Button::new("reset-native")
                                .label("重置此范围")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.save(true, cx))),
                        )
                    })
                    .child(
                        Button::new("cancel-native")
                            .ghost()
                            .label("取消编辑")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.edit = None;
                                cx.notify();
                            })),
                    ),
            );
        } else {
            body = body.child(self.render_special(cx));
            if !["常规", "插件"].contains(&self.category) {
                body = body.child(Input::new(&self.search));
                let query = self.search.read(cx).value().to_lowercase();
                let keys = self
                    .catalog
                    .entries
                    .iter()
                    .filter(|(key, meta)| {
                        (settings::category(key) == self.category || !query.is_empty())
                            && (!self.project || !key.starts_with("auth.broker."))
                            && (query.is_empty()
                                || key.to_lowercase().contains(&query)
                                || meta["description"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_lowercase()
                                    .contains(&query))
                    })
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>();
                let count = keys.len();
                body = body.child(
                    uniform_list(
                        "setting-rows",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|ix| this.setting_row(&keys[ix], cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .flex_1()
                    .min_h_0(),
                );
            }
        }
        content=content.child(body).child(div().p_3().border_t_1().border_color(theme.border).text_xs().text_color(theme.muted_foreground)
            .child(if self.busy{"正在保存…".to_string()}else if self.loading{"正在后台刷新配置…".to_string()}else{self.notice.clone()})
            .child(div().mt_2().child("OMP 持续更新，GUI 未覆盖的设置、认证及高级管理可在 OMP 终端中配置。项目设置以会话执行目录为准；环境变量与启动参数可能优先于文件。")));
        div()
            .flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(nav)
            .child(content)
    }
}

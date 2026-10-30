use super::*;

impl SettingsView {
    pub(super) fn prepare_controls(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        for (key, meta) in &self.catalog.entries {
            if meta["type"] != "enum" {
                continue;
            }
            let query = self.search.read(cx).value().to_lowercase();
            if settings::category(key) != self.category
                && (query.is_empty() || !key.to_lowercase().contains(&query))
            {
                continue;
            }
            let mut values = meta["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            let selected = layer
                .get(key)
                .and_then(|v| v["value"].as_str())
                .or_else(|| meta["value"].as_str())
                .map(str::to_owned);
            if let Some(value) = &selected
                && !values.contains(value)
            {
                values.push(value.clone());
            }
            if let Some(state) = self.enums.get(key) {
                state.update(cx, |state, cx| {
                    state.set_items(values, w, cx);
                    if let Some(v) = &selected {
                        state.set_selected_value(v, w, cx);
                    }
                });
            } else {
                let state = cx.new(|cx| SelectState::new(values, None, w, cx));
                state.update(cx, |s, cx| {
                    if let Some(v) = &selected {
                        s.set_selected_value(v, w, cx);
                    }
                });
                let name = key.clone();
                self._subscriptions
                    .push(cx.subscribe(&state, move |this, _, e, cx| {
                        if let SelectEvent::Confirm(Some(v)) = e {
                            this.inline_save(name.clone(), Some(Value::String(v.clone())), cx);
                        }
                    }));
                self.enums.insert(key.clone(), state);
            }
        }
    }
    pub(super) fn inline_save(
        &mut self,
        key: String,
        value: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        if self.pending.contains(&key) {
            return;
        }
        self.pending.insert(key.clone());
        if let Some(value) = &value {
            self.pending_values.insert(key.clone(), value.clone());
        }
        let tx = self.tx.clone();
        let project = self.project;
        let omp = self.omp.clone();
        let cwd = self.cwd.clone();
        let path = settings::config_path(&self.base());
        std::thread::spawn(move || {
            let result = if project {
                settings::set_project(&path, &key, value.clone())
            } else {
                match &value {
                    Some(v) => settings::command(
                        &omp,
                        &cwd,
                        &["config", "set", &key, &v.to_string(), "--json"],
                    )
                    .map(|_| ()),
                    None => settings::command(&omp, &cwd, &["config", "reset", &key, "--json"])
                        .map(|_| ()),
                }
            };
            let _ = tx.send(ResultEvent::Inline(
                key,
                value,
                project,
                result.map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
    pub(super) fn setting_row(&self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let meta = &self.catalog.entries[key];
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        let configured = layer.get(key);
        let value = self.pending_values.get(key).or_else(|| {
            configured
                .and_then(|v| v.get("value"))
                .or_else(|| meta.get("value"))
        });
        let busy = self.pending.contains(key);
        let name = key.to_string();
        let control = if meta["type"] == "boolean" {
            Switch::new(SharedString::from(format!("toggle-{key}")))
                .checked(value.and_then(Value::as_bool).unwrap_or(false))
                .disabled(busy)
                .on_click(cx.listener(move |this, v: &bool, _, cx| {
                    this.inline_save(name.clone(), Some(Value::Bool(*v)), cx)
                }))
                .into_any_element()
        } else if let Some(state) = self.enums.get(key) {
            div()
                .w(rems(13.))
                .h(px(32.))
                .flex_shrink_0()
                .child(Select::new(state).w_full().disabled(busy))
                .into_any_element()
        } else {
            Button::new(SharedString::from(format!("edit-{key}")))
                .ghost()
                .small()
                .label("编辑")
                .accessibility_label(format!("编辑 {key}"))
                .disabled(busy)
                .on_click(cx.listener(move |this, _, w, cx| this.key(name.clone(), w, cx)))
                .into_any_element()
        };
        let reset = key.to_string();
        let description = meta["description"].as_str().unwrap_or("").to_owned();
        div()
            .id(SharedString::from(format!("setting-row-{key}")))
            .w_full()
            .h(px(112.))
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().truncate().child(key.to_string()))
                    .child(
                        div()
                            .id("description")
                            .text_xs()
                            .h(px(34.))
                            .overflow_hidden()
                            .text_color(cx.theme().muted_foreground)
                            .tooltip(move |w, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(description.clone())
                                    .build(w, cx)
                            })
                            .child(meta["description"].as_str().unwrap_or("").to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if configured.is_some() {
                                "此范围已配置"
                            } else {
                                "继承 / 默认 / 环境"
                            }),
                    ),
            )
            .child(
                div()
                    .w(rems(13.))
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(control),
            )
            .child(
                Button::new(SharedString::from(format!("reset-{key}")))
                    .ghost()
                    .xsmall()
                    .icon(IconName::RotateCw)
                    .tooltip("重置此范围")
                    .disabled(configured.is_none() || busy)
                    .on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.inline_save(reset.clone(), None, cx)
                        }),
                    ),
            )
            .into_any_element()
    }
}

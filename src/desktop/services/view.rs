use super::*;

impl Render for RegistryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut body = div()
            .id("registry-body")
            .flex_1()
            .min_h_0()
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if self.web {
            body = body.child(self.search_directory(cx));
            body=body.child(v_form().layout(Axis::Horizontal)
                .child(field().label("启用网络搜索").child(div().w_full().flex().justify_end().child(Switch::new("enable-web").checked(self.web_enabled).on_click(cx.listener(|this,v:&bool,_,cx|{this.web_enabled= *v;cx.notify();})))))
                .child(field().label("首选搜索服务").child(Select::new(&self.search_service)))
                .child(field().label("API 密钥 · 全局凭据").description("留空沿用 OMP 已有凭据；新密钥通过 OMP 原生凭据库保存。无需密钥的服务可留空。").child(Input::new(&self.fields["search_key"]).mask_toggle()))
                .child(field().label("SearXNG 地址").description("仅选择 SearXNG 时使用。").child(Input::new(&self.fields["search_url"])))
                .child(field().label("SearXNG Token").description("留空保留；按当前全局／项目范围保存。").child(Input::new(&self.fields["search_token"]).mask_toggle())));
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child("备用搜索服务 · 按顺序尝试"))
                    .child(
                        Button::new("add-search-fallback")
                            .small()
                            .label("加入所选服务")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(v) =
                                    this.search_service.read(cx).selected_value().cloned()
                                    && !this.fallbacks.contains(&v)
                                {
                                    this.fallbacks.push(v);
                                }
                                cx.notify();
                            })),
                    ),
            );
            for (ix, name) in self.fallbacks.iter().enumerate() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(name.clone()))
                        .child(
                            Button::new(SharedString::from(format!("web-up-{ix}")))
                                .ghost()
                                .small()
                                .label("上移")
                                .disabled(ix == 0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.fallbacks.swap(ix, ix - 1);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("web-remove-{ix}")))
                                .ghost()
                                .small()
                                .label("移除")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.fallbacks.remove(ix);
                                    cx.notify();
                                })),
                        ),
                );
            }
            body = body.child(
                Button::new("save-web")
                    .primary()
                    .label("保存搜索配置")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.save_web(cx))),
            );
        } else if !self.editing {
            body = body.child(self.directory(cx));
        } else if self.model_form {
            body = body.child(self.model_form(cx));
        } else {
            body = body.child(
                v_form()
                    .child(
                        field()
                            .label("服务标识")
                            .description("OMP 使用此标识关联模型和角色。")
                            .child(
                                Input::new(&self.fields["provider"])
                                    .disabled(self.selected.is_some()),
                            ),
                    )
                    .child(
                        field()
                            .label("API 根地址")
                            .child(Input::new(&self.fields["url"])),
                    )
                    .child(field().label("API 格式").child(Select::new(&self.api)))
                    .child(
                        field()
                            .label("API 密钥或环境变量名称")
                            .description(
                                if self.draft.get("apiKey").is_some()
                                    || self.selected.as_ref().is_some_and(|id| {
                                        self.services.registered_providers().contains(id)
                                    })
                                {
                                    "已配置，留空保留现有凭据。"
                                } else {
                                    "本地免密服务可留空；也可填写 OMP 可读取的环境变量名称。"
                                },
                            )
                            .child(Input::new(&self.fields["key"]).mask_toggle()),
                    ),
            );
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("discover-models")
                            .label("获取服务模型")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.discover(cx))),
                    )
                    .child(Button::new("add-model").label("手动添加模型").on_click(
                        cx.listener(|this, _, w, cx| this.edit_model(None, None, w, cx)),
                    )),
            );
            if !self.discovered.is_empty() {
                body = body
                    .child(
                        v_form().child(
                            field()
                                .label("服务提供的模型")
                                .child(Select::new(&self.model_picker)),
                        ),
                    )
                    .child(
                        Button::new("add-discovered-model")
                            .label("添加所选模型")
                            .on_click(cx.listener(|this, _, w, cx| {
                                if let Some(id) =
                                    this.model_picker.read(cx).selected_value().cloned()
                                {
                                    this.edit_model(None, Some(id), w, cx);
                                } else {
                                    this.notice = "请先在列表中选择模型".into();
                                    cx.notify();
                                }
                            })),
                    );
            }
            body = body.child(div().mt_2().child("已选择的模型"));
            body = body.child(
                div()
                    .relative()
                    .h_64()
                    .flex_shrink_0()
                    .child(
                        uniform_list(
                            "provider-models",
                            self.models.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                let theme = cx.theme().clone();
                                range
                                    .map(|ix| {
                                        let model = &this.models[ix];

                                        div()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .h_16()
                                            .border_b_1()
                                            .border_color(theme.border)
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(
                                                        model["id"]
                                                            .as_str()
                                                            .unwrap_or("")
                                                            .to_string(),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!(
                                                                "上下文 {} · 输出 {}",
                                                                model["contextWindow"],
                                                                model["maxTokens"]
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "model-edit-{ix}"
                                                )))
                                                .ghost()
                                                .small()
                                                .label("参数")
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.edit_model(Some(ix), None, w, cx)
                                                })),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "model-delete-{ix}"
                                                )))
                                                .ghost()
                                                .small()
                                                .label(
                                                    if this
                                                        .provider_draft
                                                        .native_models
                                                        .contains_key(
                                                            model["id"]
                                                                .as_str()
                                                                .unwrap_or_default(),
                                                        )
                                                    {
                                                        "内置"
                                                    } else {
                                                        "移除"
                                                    },
                                                )
                                                .disabled(
                                                    this.provider_draft.native_models.contains_key(
                                                        model["id"].as_str().unwrap_or_default(),
                                                    ),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.models.remove(ix);
                                                    cx.notify();
                                                })),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .track_scroll(&self.models_scroll)
                        .h_full(),
                    )
                    .vertical_scrollbar(&self.models_scroll),
            );
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .mt_3()
                    .child(
                        Button::new("save-provider")
                            .primary()
                            .label("保存服务与模型")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save_provider(false, cx))),
                    )
                    .child(
                        Button::new("cancel-provider")
                            .ghost()
                            .label("取消")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editing = false;
                                cx.notify();
                            })),
                    )
                    .when(
                        self.selected
                            .as_ref()
                            .is_some_and(|id| self.providers.values.contains_key(id)),
                        |d| {
                            d.child(
                                Button::new("delete-provider")
                                    .ghost()
                                    .label(if self.delete_confirm {
                                        "确认移除该服务配置"
                                    } else {
                                        "移除服务"
                                    })
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.delete_confirm {
                                            this.save_provider(true, cx);
                                        } else {
                                            this.delete_confirm = true;
                                            cx.notify();
                                        }
                                    })),
                            )
                        },
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("back-registry")
                            .ghost()
                            .label("返回设置")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(RegistryEvent::Close))),
                    )
                    .child(div().text_lg().child(if self.web {
                        "网络搜索服务"
                    } else {
                        "模型服务与注册"
                    })),
            )
            .child(body.overflow_y_scrollbar())
            .child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(if self.busy || self.catalog_loading {
                        "正在处理…".to_string()
                    } else {
                        self.notice.clone()
                    }),
            )
    }
}

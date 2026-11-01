use super::*;
use gpui_kit::component::form::{field, v_form};

struct RoleRow {
    name: String,
    initial: Option<String>,
    picker: Entity<ModelPicker>,
}
pub(in crate::desktop::settings) struct RoleEditor {
    draft: authoring::RoleDraft,
    configuration: Configuration,
    rows: Vec<RoleRow>,
    models: Vec<Value>,
    new_name: Entity<InputState>,
    busy: bool,
    notice: String,
    tx: mpsc::Sender<EditorResult>,
    rx: mpsc::Receiver<EditorResult>,
}
impl EventEmitter<EditorEvent> for RoleEditor {}

impl RoleEditor {
    pub(in crate::desktop::settings) fn new(
        draft: authoring::RoleDraft,
        configuration: Configuration,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut names = authoring::role_names(&draft.effective);
        for name in draft.values.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let rows = names
            .into_iter()
            .map(|name| {
                let initial = draft
                    .values
                    .get(&name)
                    .and_then(|v| v.as_str().or_else(|| v.as_array()?.first()?.as_str()))
                    .map(str::to_owned);
                let picker = cx.new(|cx| {
                    ModelPicker::new(
                        name.clone(),
                        draft.effective.clone(),
                        false,
                        initial.as_deref(),
                        w,
                        cx,
                    )
                });
                RoleRow {
                    name,
                    initial,
                    picker,
                }
            })
            .collect();
        let (tx, rx) = channel(configuration.clone());
        cx.spawn_in(w, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if view
                    .update_in(cx, |this, w, cx| {
                        for event in this.rx.try_iter().collect::<Vec<_>>() {
                            match event {
                                EditorResult::Models(Ok(models)) => {
                                    this.models = models;
                                    for row in &this.rows {
                                        row.picker.update(cx, |s, cx| {
                                            s.set_models(this.models.clone(), w, cx)
                                        });
                                    }
                                    if this.notice == "正在读取可用模型…" {
                                        this.notice.clear();
                                    }
                                }
                                EditorResult::Saved(Ok(())) => {
                                    this.busy = false;
                                    cx.emit(EditorEvent::Saved);
                                }
                                EditorResult::Models(Err(e)) => {
                                    this.notice = e;
                                }
                                EditorResult::Saved(Err(e)) => {
                                    this.busy = false;
                                    this.notice = e;
                                }
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            draft,
            configuration,
            rows,
            models: Vec::new(),
            new_name: cx.new(|cx| InputState::new(w, cx).placeholder("例如 review、fast_worker")),
            busy: false,
            notice: "正在读取可用模型…".into(),
            tx,
            rx,
        }
    }
    fn add_role(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_owned();
        if let Err(e) = settings::native_name(&name) {
            self.notice = e.to_string();
            cx.notify();
            return;
        }
        if self.rows.iter().any(|r| r.name == name) {
            self.notice = "此角色已存在，请在下方选择模型".into();
            cx.notify();
            return;
        }
        let picker = cx.new(|cx| {
            ModelPicker::new(
                name.clone(),
                self.draft.effective.clone(),
                false,
                None,
                w,
                cx,
            )
        });
        picker.update(cx, |s, cx| s.set_models(self.models.clone(), w, cx));
        self.rows.push(RoleRow {
            name,
            initial: None,
            picker,
        });
        self.new_name.update(cx, |s, cx| s.set_value("", w, cx));
        self.notice.clear();
        cx.notify();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let mut values = self.draft.values.clone();
        for row in &self.rows {
            let value = row.picker.read(cx).value(cx);
            if value == row.initial {
                continue;
            }
            if let Some(value) = value {
                values.insert(row.name.clone(), Value::String(value));
            } else {
                values.remove(&row.name);
            }
        }
        let (draft, tx) = (self.draft.clone(), self.tx.clone());
        let configuration = self.configuration.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(EditorResult::Saved(
                configuration
                    .call(ConfigAction::SaveRoles {
                        draft: Box::new(draft),
                        values,
                    })
                    .map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
}

impl Render for RoleEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut form = v_form();
        for row in &self.rows {
            let inherited = self
                .draft
                .effective
                .get(&row.name)
                .filter(|_| !self.draft.values.contains_key(&row.name));
            form = form.child(
                field()
                    .label(row.name.clone())
                    .description(
                        inherited
                            .map(|value| format!("当前继承：{value}"))
                            .unwrap_or_else(|| "选择模型；继承／默认会移除此范围的覆盖。".into()),
                    )
                    .child(row.picker.clone()),
            );
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().text_lg().child("角色与模型关联"))
                    .child(
                        Button::new("cancel-roles")
                            .ghost()
                            .label("返回")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Close))),
                    )
                    .child(
                        Button::new("save-roles")
                            .primary()
                            .label("保存关联")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
            .child(
                div()
                    .id("role-model-form")
                    .flex_1()
                    .min_h_0()
                    .p_5()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .mb_3()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("保存范围：{}", self.draft.path.display())),
                    )
                    .child(
                        div()
                            .mb_3()
                            .text_sm()
                            .text_color(if self.notice == "正在读取可用模型…" {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().danger
                            })
                            .child(self.notice.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .mb_4()
                            .child(Input::new(&self.new_name).aria_label("自定义角色名称"))
                            .child(
                                Button::new("add-role")
                                    .label("添加角色")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, w, cx| this.add_role(w, cx))),
                            ),
                    )
                    .child(form),
            )
    }
}

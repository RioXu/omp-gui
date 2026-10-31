use super::*;
use gpui_kit::component::form::{field, v_form};

pub(in crate::desktop::settings) struct AgentEditor {
    base: PathBuf,
    draft: authoring::AgentDraft,
    name: Entity<InputState>,
    description: Entity<InputState>,
    prompt: Entity<TextareaState>,
    model: Entity<ModelPicker>,
    busy: bool,
    notice: String,
    tx: mpsc::Sender<EditorResult>,
    rx: mpsc::Receiver<EditorResult>,
}
impl EventEmitter<EditorEvent> for AgentEditor {}

impl AgentEditor {
    pub(in crate::desktop::settings) fn new(
        base: PathBuf,
        draft: authoring::AgentDraft,
        omp: String,
        cwd: PathBuf,
        aliases: Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tx, rx) = channel(omp, cwd);
        let name = cx.new(|cx| {
            InputState::new(w, cx)
                .placeholder("例如 reviewer")
                .default_value(draft.name.clone())
        });
        let description = cx.new(|cx| {
            InputState::new(w, cx)
                .placeholder("这个子智能体负责什么任务")
                .default_value(draft.description.clone())
        });
        let prompt = cx.new(|cx| {
            TextareaState::new(w, cx)
                .auto_grow(8, 20)
                .placeholder("编写子智能体的职责、工作方式和输出要求")
                .default_value(draft.prompt.clone())
        });
        let model = cx.new(|cx| {
            ModelPicker::new(
                "default".into(),
                aliases,
                true,
                draft.model.as_deref(),
                w,
                cx,
            )
        });
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
                                    this.model.update(cx, |s, cx| s.set_models(models, w, cx));
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
            base,
            draft,
            name,
            description,
            prompt,
            model,
            busy: false,
            notice: "正在读取可用模型…".into(),
            tx,
            rx,
        }
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name.read(cx).value().trim().to_owned();
        let description = self.description.read(cx).value().trim().to_owned();
        let prompt = self.prompt.read(cx).value().to_string();
        let model = self.model.read(cx).value(cx);
        let text = match self
            .draft
            .document(&name, &description, &prompt, model.as_deref())
        {
            Ok(text) => text,
            Err(e) => {
                self.notice = e.to_string();
                cx.notify();
                return;
            }
        };
        let (draft, base, tx) = (self.draft.clone(), self.base.clone(), self.tx.clone());
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(EditorResult::Saved(
                draft
                    .save(&base, &name, &text)
                    .map_err(|e| format!("{e:#}")),
            ));
        });
        cx.notify();
    }
}
impl Render for AgentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(
                        div()
                            .flex_1()
                            .text_lg()
                            .child(if self.draft.name.is_empty() {
                                "新建子智能体"
                            } else {
                                "编辑子智能体"
                            }),
                    )
                    .child(
                        Button::new("cancel-agent")
                            .ghost()
                            .label("返回")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Close))),
                    )
                    .child(
                        Button::new("save-agent")
                            .primary()
                            .label("保存子智能体")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
            .child(
                div()
                    .id("agent-form")
                    .flex_1()
                    .min_h_0()
                    .p_5()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .mb_3()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("保存位置：{}", self.base.join("agents").display())),
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
                        v_form()
                            .child(
                                field()
                                    .label("名称")
                                    .description(
                                        "仅使用字母、数字、- 或 _；main 和 sub 为保留名称。",
                                    )
                                    .child(Input::new(&self.name).aria_label("子智能体名称")),
                            )
                            .child(field().label("职责说明").child(
                                Input::new(&self.description).aria_label("子智能体职责说明"),
                            ))
                            .child(
                                field()
                                    .label("模型")
                                    .description(
                                        "可继承父会话，选择已配置模型，或通过 @角色 使用角色关联。",
                                    )
                                    .child(self.model.clone()),
                            )
                            .child(field().label("系统提示词").child(
                                Textarea::new(&self.prompt).aria_label("子智能体系统提示词"),
                            )),
                    ),
            )
    }
}

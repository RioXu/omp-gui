use super::*;
use gpui_kit::component::select::SelectEvent;

const INHERIT: &str = "继承／默认";
const DEFAULT_EFFORT: &str = "沿用模型设置";

pub(super) struct ModelPicker {
    model: Entity<SelectState<Vec<String>>>,
    effort: Entity<SelectState<Vec<String>>>,
    models: Vec<Value>,
    role: String,
    aliases: Value,
    allow_aliases: bool,
    _subscriptions: Vec<Subscription>,
}

impl ModelPicker {
    pub(super) fn new(
        role: String,
        aliases: Value,
        allow_aliases: bool,
        value: Option<&str>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (base, level) = authoring::split_effort(value.unwrap_or(INHERIT));
        let model = cx.new(|cx| {
            SelectState::new(vec![INHERIT.to_owned(), base.to_owned()], None, w, cx)
                .searchable(true)
        });
        model.update(cx, |s, cx| s.set_selected_value(&base.to_owned(), w, cx));
        let effort = cx.new(|cx| {
            SelectState::new(
                vec![DEFAULT_EFFORT.to_owned(), level.to_owned()],
                None,
                w,
                cx,
            )
        });
        effort.update(cx, |s, cx| {
            s.set_selected_value(
                &if level.is_empty() {
                    DEFAULT_EFFORT.into()
                } else {
                    level.into()
                },
                w,
                cx,
            )
        });
        let subscription = cx.subscribe_in(&model, w, |this, _, event, w, cx| {
            if matches!(event, SelectEvent::Confirm(_)) {
                this.sync_effort(true, w, cx);
            }
        });
        let mut this = Self {
            model,
            effort,
            models: Vec::new(),
            role,
            aliases,
            allow_aliases,
            _subscriptions: vec![subscription],
        };
        this.set_models(Vec::new(), w, cx);
        this
    }
    pub(super) fn value(&self, cx: &App) -> Option<String> {
        let model = self.model.read(cx).selected_value()?;
        if model == INHERIT {
            return None;
        }
        let effort = self
            .effort
            .read(cx)
            .selected_value()
            .map(String::as_str)
            .unwrap_or(DEFAULT_EFFORT);
        Some(if effort == DEFAULT_EFFORT {
            model.clone()
        } else {
            format!("{model}:{effort}")
        })
    }
    pub(super) fn set_models(
        &mut self,
        models: Vec<Value>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.models = models;
        let current = self
            .model
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(INHERIT.into());
        let mut options = vec![INHERIT.into()];
        if self.allow_aliases {
            options.extend(
                authoring::role_names(&self.aliases)
                    .into_iter()
                    .filter(|r| {
                        !["web", "image", "speech", "dictation", "judge"].contains(&r.as_str())
                    })
                    .map(|r| format!("@{r}")),
            );
        }
        options.extend(authoring::choices(&self.models, &self.role));
        if !options.contains(&current) {
            options.push(current.clone());
        }
        self.model.update(cx, |s, cx| {
            s.set_items(options, w, cx);
            s.set_selected_value(&current, w, cx);
        });
        self.sync_effort(false, w, cx);
    }
    fn sync_effort(&mut self, reset: bool, w: &mut Window, cx: &mut Context<Self>) {
        let selected = self
            .model
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_default();
        let resolved = selected
            .strip_prefix('@')
            .and_then(|r| self.aliases.get(r))
            .and_then(Value::as_str)
            .unwrap_or(&selected);
        let mut options = vec![DEFAULT_EFFORT.to_owned()];
        options.extend(authoring::effort_options(
            &self.models,
            authoring::split_effort(resolved).0,
        ));
        let current = if reset {
            DEFAULT_EFFORT.into()
        } else {
            self.effort
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or(DEFAULT_EFFORT.into())
        };
        if !options.contains(&current) {
            options.push(current.clone());
        }
        self.effort.update(cx, |s, cx| {
            s.set_items(options, w, cx);
            s.set_selected_value(&current, w, cx);
        });
        cx.notify();
    }
}
impl Render for ModelPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .min_w_0()
            .child(Select::new(&self.model).w_full())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("推理强度"),
                    )
                    .child(
                        Select::new(&self.effort).w(rems(14.)).disabled(
                            self.model
                                .read(cx)
                                .selected_value()
                                .is_none_or(|v| v == INHERIT),
                        ),
                    ),
            )
    }
}

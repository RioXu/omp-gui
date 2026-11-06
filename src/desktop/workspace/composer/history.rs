use super::*;

impl Workspace {
    pub(in crate::desktop::workspace) fn input_history_available(&self, cx: &App) -> bool {
        let state = &self.runtime.composer_state;
        let value = state.input.read(cx).value();
        value.is_empty()
            || state
                .history_cursor
                .and_then(|ix| state.history.get(ix))
                .is_some_and(|text| text.as_str() == value.as_str())
    }

    pub(in crate::desktop::workspace) fn browse_input_history(
        &mut self,
        older: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_history_available(cx) || !self.completions(cx).is_empty() {
            return;
        }
        let value = self.runtime.composer_state.input.read(cx).value();
        if value.is_empty() {
            if !older {
                return;
            }
            let mut history: Vec<_> = self
                .session()
                .into_iter()
                .flat_map(|s| s.messages.iter().rev())
                .filter(|m| m.role == "user" && !m.text.trim().is_empty())
                .map(|m| m.text.clone())
                .collect();
            history.dedup();
            self.runtime.composer_state.history = history;
            self.runtime.composer_state.history_cursor = None;
        }
        let state = &mut self.runtime.composer_state;
        if state.history.is_empty() {
            return;
        }
        let at = if older {
            Some(
                state
                    .history_cursor
                    .map_or(0, |ix| (ix + 1).min(state.history.len() - 1)),
            )
        } else {
            state.history_cursor.and_then(|ix| ix.checked_sub(1))
        };
        state.history_cursor = at;
        state.dismissed = true;
        let text = at.map(|ix| state.history[ix].clone()).unwrap_or_default();
        state
            .input
            .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
        if let Some(session) = self.session_mut() {
            session.draft = text;
        }
        self.dirty = true;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::workspace::runtime::sessions::tests::fixture;
    use core::prelude::v1::test;
    use gpui_kit::{TestAppContext, VisualTestContext};

    #[gpui_kit::test]
    fn arrow_keys_recall_only_session_inputs_and_preserve_edited_drafts(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new(
                    "up",
                    PreviousInput,
                    Some("Workspace > ComposerHistory > Input"),
                ),
                KeyBinding::new(
                    "down",
                    NextInput,
                    Some("Workspace > ComposerHistory > Input"),
                ),
            ])
        });
        let (window, view) = cx.update(|cx| {
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut this = fixture(window, cx);
                    this.db.sessions[0].messages = vec![
                        Message::new("user", "第一条".into()),
                        Message::new("assistant", "not input".into()),
                        Message::new("user", "第二条\n多行".into()),
                    ];
                    this.db.sessions[0].messages[2].native_text =
                        Some("augmented wire prompt".into());
                    this.runtime
                        .composer_state
                        .input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.runtime
                        .composer_state
                        .input
                        .focus_handle(cx)
                        .focus(window, cx);
                    this
                })
            })
            .unwrap()
        });
        let mut visual = VisualTestContext::from_window(window, cx);
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        for (key, expected) in [
            ("up", "第二条\n多行"),
            ("up", "第一条"),
            ("down", "第二条\n多行"),
            ("down", ""),
        ] {
            visual.simulate_keystrokes(key);
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            view.read_with(&visual, |this, cx| {
                assert_eq!(
                    this.runtime.composer_state.input.read(cx).value().as_str(),
                    expected
                )
            });
        }
        visual.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.runtime.composer_state.input.update(cx, |input, cx| {
                    input.set_value("正在编辑\n第二行", window, cx)
                });
                this.browse_input_history(true, window, cx);
                assert_eq!(
                    this.runtime.composer_state.input.read(cx).value().as_str(),
                    "正在编辑\n第二行"
                );
                this.activate(1, window, cx);
                this.runtime
                    .composer_state
                    .input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                this.browse_input_history(true, window, cx);
                assert_eq!(
                    this.runtime.composer_state.input.read(cx).value().as_str(),
                    "B"
                );
            })
        });
    }
}

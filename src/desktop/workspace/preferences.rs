use super::*;
use crate::desktop::theme::apply_accent;

impl Workspace {
    pub fn answer_request(&mut self, value: Option<String>, confirmed: Option<bool>) {
        if let Some(r) = self.requests.pop_front() {
            let mut f = json!({"type":"extension_ui_response","id":r["id"]});
            if let Some(v) = value {
                f["value"] = json!(v);
            } else if let Some(v) = confirmed {
                f["confirmed"] = json!(v);
            } else {
                f["cancelled"] = json!(true);
            }
            self.command(f);
        }
    }
    pub fn settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_disabled {
            return;
        }
        let cwd = self
            .session()
            .map(|s| s.cwd().to_owned())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let view = cx.new(|cx| {
            crate::desktop::settings::SettingsView::new(
                self.db.omp_path.clone(),
                cwd,
                self.session().map(|s| s.project.clone()),
                window,
                cx,
            )
        });
        self._subscriptions
            .push(cx.subscribe(&view, |this, _, event, cx| {
                match event {
                    crate::desktop::settings::SettingsEvent::OmpPath(path) => {
                        this.db.omp_path = path.clone();
                        this.persist();
                        if !this.running {
                            this.engine = None;
                            this.connected = false;
                            this.starting = false;
                            this.session_sync.store = None;
                            this.scan_sessions();
                        }
                    }
                    crate::desktop::settings::SettingsEvent::Saved => {
                        this.request_state();
                        this.command(json!({"type":"get_available_models","id":"models"}));
                        this.request_thinking_levels();
                        this.request_thinking_source();
                    }
                    crate::desktop::settings::SettingsEvent::Reconnect => {
                        if !this.running {
                            this.connect();
                        }
                    }
                }
                cx.notify();
            }));
        self.settings_view = Some(view);
        cx.notify();
    }
    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dark = !cx.theme().is_dark();
        Theme::change(
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        self.db.theme = if dark { "dark" } else { "light" }.into();
        apply_accent(cx);
        self.persist();
    }
}

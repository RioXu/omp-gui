use super::*;
use crate::desktop::theme::apply_accent;

impl Workspace {
    pub fn answer_request(&mut self, value: Option<String>, confirmed: Option<bool>) {
        if !self.runtime.connected && !self.demo {
            self.runtime.error = Some("连接恢复后才能回答此交互请求".into());
            return;
        }
        if let Some(r) = self.runtime.requests.pop_front() {
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
        let cwd = self.session().map(Session::host_cwd).unwrap_or_else(|| {
            if self.db.active_environment == "local" {
                omp_core::environment::HostPath::Local(std::env::current_dir().unwrap_or_default())
            } else {
                omp_core::environment::HostPath::Posix("/".into())
            }
        });
        let host = match self.execution_host() {
            Ok(host) => host,
            Err(error) => {
                self.runtime.error = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        let configuration = omp_core::host::Configuration {
            host,
            omp: self.environment_omp(),
            cwd,
        };
        let environment = self.db.active_environment.clone();
        let view = cx.new(|cx| {
            crate::desktop::settings::SettingsView::new(
                configuration,
                self.session()
                    .map(|s| PathBuf::from(s.project_location().path.text())),
                window,
                cx,
            )
        });
        self._subscriptions.push(cx.subscribe_in(
            &view,
            window,
            move |this, _, event, window, cx| {
                match event {
                    crate::desktop::settings::SettingsEvent::OmpPath(path) => {
                        if environment == "local" {
                            this.db.omp_path = path.clone();
                        }
                        if let Some(saved) = this
                            .db
                            .environments
                            .iter_mut()
                            .find(|saved| saved.id == environment)
                        {
                            saved.omp_override = (path != "omp").then(|| path.clone());
                        }
                        this.persist();
                        if this.db.active_environment == environment && !this.runtime.running {
                            this.runtime.engine = None;
                            this.runtime.connected = false;
                            this.runtime.starting = false;
                            this.session_sync.store = None;
                            this.scan_sessions();
                        }
                    }
                    crate::desktop::settings::SettingsEvent::Saved => {
                        if this.db.active_environment != environment {
                            return;
                        }
                        this.request_state();
                        this.command(json!({"type":"get_available_models","id":"models"}));
                        this.request_thinking_levels();
                        this.request_thinking_source();
                    }
                    crate::desktop::settings::SettingsEvent::Reconnect => {
                        if this.db.active_environment == environment && !this.runtime.running {
                            this.connect();
                        }
                    }
                    crate::desktop::settings::SettingsEvent::ManageConnections => {
                        this.manage_connection(None, window, cx)
                    }
                }
                cx.notify();
            },
        ));
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

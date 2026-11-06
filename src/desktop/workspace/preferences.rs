use super::*;
use crate::desktop::theme::{self, Skin};

impl Workspace {
    pub fn answer_request(
        &mut self,
        value: Option<String>,
        confirmed: Option<bool>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.runtime.connected && !self.demo {
            self.runtime.error = Some("连接恢复后才能回答此交互请求".into());
            return false;
        }
        if let Some(r) = self.runtime.requests.front().cloned() {
            let mut f = json!({"type":"extension_ui_response","id":r["id"]});
            if let Some(v) = &value {
                f["value"] = json!(v);
            } else if let Some(v) = confirmed {
                f["confirmed"] = json!(v);
            } else {
                f["cancelled"] = json!(true);
            }
            if !self.demo {
                let sent = self
                    .runtime
                    .engine
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OMP 连接不可用"))
                    .and_then(|engine| engine.send(f));
                if let Err(error) = sent {
                    self.runtime.error = Some(format!("回答发送失败：{error}"));
                    return false;
                }
            }
            self.runtime.requests.pop_front();
            if matches!(r["method"].as_str(), Some("input" | "editor"))
                && r["secret"] != true
                && let Some(text) = value.filter(|v| !v.trim().is_empty())
                && let Some(session) = self.session_mut()
                && !session.messages.iter().any(|m| {
                    m.is_interaction_reply()
                        && m.details["omp_gui_interaction"]["request_id"] == r["id"]
                })
            {
                let mut message = Message::new("user", text);
                message.details = json!({"omp_gui_interaction": {"request_id":r["id"], "method":r["method"], "title":r["title"]}});
                session.messages.push(message);
                self.sync_texts(cx);
                self.persist();
            }
            if self.runtime.requests.is_empty() {
                self.runtime.computer_overlay.resume();
            }
            return true;
        }
        false
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
    pub fn choose_skin(&mut self, skin: Skin, window: &mut Window, cx: &mut Context<Self>) {
        theme::apply(skin, Some(window), cx);
        self.db.theme = skin.id().into();
        self.persist();
        cx.notify();
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::desktop::workspace::runtime::sessions::tests::fixture;
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    struct Capture {
        frames: Arc<Mutex<Vec<Value>>>,
        fail: Arc<AtomicBool>,
    }
    impl crate::infrastructure::omp::SessionConnection for Capture {
        fn send(&self, frame: Value) -> anyhow::Result<()> {
            if self.fail.load(Ordering::Relaxed) {
                anyhow::bail!("test disconnect");
            }
            self.frames.lock().unwrap().push(frame);
            Ok(())
        }
        fn poll(&self, _: usize) -> Vec<crate::infrastructure::omp::Event> {
            vec![]
        }
    }

    #[gpui_kit::test]
    fn other_reply_is_visible_persistent_and_survives_native_history_refresh(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| fixture(window, cx))
            })
            .unwrap()
        });
        window.update(cx, |_, window, cx| view.update(cx, |this, cx| {
            let frames = Arc::new(Mutex::new(vec![]));
            this.demo = false;
            this.runtime.connected = true;
            this.runtime.engine = Some(Box::new(Capture { frames: frames.clone(), fail: Arc::new(AtomicBool::new(false)) }));
            let question = Message::new("assistant", "选择一种方案".into());
            this.session_mut().unwrap().messages = vec![question.clone()];
            this.on_frame(json!({"type":"extension_ui_request","id":"choose","method":"select","options":["A","Other"]}), window, cx);
            assert!(this.answer_request(Some("Other".into()), None, cx));
            assert_eq!(this.session().unwrap().messages.len(), 1);
            this.on_frame(json!({"type":"extension_ui_request","id":"custom","method":"input","title":"Other"}), window, cx);
            let text = "我的自定义方案\n保留中文与格式。";
            assert!(this.answer_request(Some(text.into()), None, cx));
            let reply = this.session().unwrap().messages[1].clone();
            assert_eq!(reply.role, "user");
            assert_eq!(this.runtime.texts[&reply.id].body, text);
            assert!(this.runtime.transcript.iter().any(|r| matches!(r, TranscriptRow::User(1))));
            assert_eq!(frames.lock().unwrap().last().unwrap()["value"], text);
            assert!(frames.lock().unwrap().iter().all(|f| f["type"] == "extension_ui_response"));
            assert!(!this.answer_request(Some(text.into()), None, cx));
            let serialized = serde_json::to_string(this.session().unwrap()).unwrap();
            let mut restored: Session = serde_json::from_str(&serialized).unwrap();
            let next = Message::new("assistant", "继续处理".into());
            for _ in 0..2 { restored.merge_native_history(vec![question.clone(), next.clone()]); }
            assert_eq!(restored.messages.len(), 3);
            assert_eq!(restored.messages[1].id, reply.id);
            assert_eq!(restored.messages[1].text, text);
            assert!(restored.messages[1].is_interaction_reply());
            assert_eq!(restored.messages[2].text, "继续处理");
        })).unwrap();
    }

    #[gpui_kit::test]
    fn failed_cancelled_and_secret_answers_do_not_create_visible_replies(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| fixture(window, cx))
            })
            .unwrap()
        });
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.demo = false;
                    this.runtime.connected = true;
                    let fail = Arc::new(AtomicBool::new(true));
                    this.runtime.engine = Some(Box::new(Capture {
                        frames: Arc::new(Mutex::new(vec![])),
                        fail: fail.clone(),
                    }));
                    this.session_mut().unwrap().messages.clear();
                    this.runtime
                        .requests
                        .push_back(json!({"id":"retry","method":"input"}));
                    assert!(!this.answer_request(Some("retry text".into()), None, cx));
                    assert_eq!(this.runtime.requests.len(), 1);
                    assert!(this.session().unwrap().messages.is_empty());
                    fail.store(false, Ordering::Relaxed);
                    assert!(this.answer_request(Some("retry text".into()), None, cx));
                    assert_eq!(this.session().unwrap().messages.len(), 1);
                    for request in [
                        json!({"id":"cancel","method":"input"}),
                        json!({"id":"secret","method":"input","secret":true}),
                    ] {
                        this.runtime.requests.push_back(request.clone());
                        let value = (request["secret"] == true).then(|| "private".into());
                        assert!(this.answer_request(value, None, cx));
                    }
                    assert_eq!(this.session().unwrap().messages.len(), 1);
                })
            })
            .unwrap();
    }
}

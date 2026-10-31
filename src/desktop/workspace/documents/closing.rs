use super::*;
use gpui_kit::component::{
    button::{Button, ButtonVariants},
    dialog::{DialogAction, DialogClose, DialogFooter},
};

#[derive(Clone)]
pub(in crate::desktop::workspace) enum LeaveAction {
    Activate(String),
    NewSession(PathBuf),
    ManageSession { id: String, delete: bool },
    CloseAll,
    CloseWindow,
}

impl Workspace {
    pub(in crate::desktop::workspace) fn clear_documents(&mut self) {
        self.document_state.subscriptions.clear();
        self.document_state.tabs.clear();
        self.activate_document(None);
    }

    pub(in crate::desktop::workspace) fn request_close_document(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_state.prompt.is_some() {
            return;
        }
        let Some(tab) = self.document_state.tabs.iter().find(|t| t.key == key) else {
            return;
        };
        if !tab.is_dirty(cx) {
            self.close_document(key);
            self.focus_document_editor(window, cx);
            cx.notify();
            return;
        }
        let name = tab
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let token = id();
        self.document_state.prompt = Some(token.clone());
        let session = self.session().map(|s| s.id.clone());
        let key = key.to_owned();
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let error = weak.upgrade().and_then(|view| {
                view.read(cx)
                    .document_state
                    .tabs
                    .iter()
                    .find(|t| t.key == key)
                    .and_then(|t| t.error.clone())
            });
            let save_weak = weak.clone();
            let save_key = key.clone();
            let save_token = token.clone();
            let save_session = session.clone();
            let discard_weak = weak.clone();
            let discard_key = key.clone();
            let discard_token = token.clone();
            let discard_session = session.clone();
            let close_weak = weak.clone();
            let close_token = token.clone();
            let dialog = dialog
                .title("未保存的修改")
                .overlay_closable(false)
                .child(format!("“{name}”有未保存的修改，是否保存？"));
            let dialog = if let Some(error) = error {
                dialog.child(div().text_sm().text_color(cx.theme().danger).child(error))
            } else {
                dialog
            };
            dialog
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogAction::new()
                                .child(Button::new("save-close-file").primary().label("保存")),
                        )
                        .child(
                            div().w_full().child(
                                Button::new("discard-file")
                                    .w_full()
                                    .label("不保存")
                                    .on_click(move |_, w, cx| {
                                        let _ = discard_weak.update(cx, |this, cx| {
                                            if this.prompt_matches(&discard_token, &discard_session)
                                            {
                                                this.document_state.prompt = None;
                                                w.close_dialog(cx);
                                                this.close_document(&discard_key);
                                                this.defer_document_focus(w, cx);
                                                cx.notify();
                                            }
                                        });
                                    }),
                            ),
                        )
                        .child(DialogClose::new().trigger(|button| button.label("取消"))),
                )
                .on_ok(move |_, w, cx| {
                    save_weak
                        .update(cx, |this, cx| {
                            if !this.prompt_matches(&save_token, &save_session) {
                                return true;
                            }
                            if !this.save_document(&save_key, w, cx) {
                                return false;
                            }
                            this.document_state.prompt = None;
                            this.close_document(&save_key);
                            cx.notify();
                            true
                        })
                        .unwrap_or(true)
                })
                .on_close(move |_, w, cx| {
                    let _ = close_weak.update(cx, |this, cx| {
                        this.clear_document_prompt(&close_token, w, cx)
                    });
                })
        });
    }

    pub(in crate::desktop::workspace) fn guard_documents(
        &mut self,
        action: LeaveAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.document_state.discard_permit {
            return true;
        }
        if self.document_state.prompt.is_some() {
            return false;
        }
        if !self.has_dirty_documents(cx) {
            return true;
        }
        let token = id();
        self.document_state.prompt = Some(token.clone());
        let session = self.session().map(|s| s.id.clone());
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let confirm = weak.clone();
            let confirm_token = token.clone();
            let confirm_session = session.clone();
            let action = action.clone();
            let close = weak.clone();
            let close_token = token.clone();
            dialog
                .title("未保存的修改")
                .overlay_closable(false)
                .child("存在未保存的文件修改。继续将丢弃这些修改，是否继续？")
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("取消")))
                        .child(
                            DialogAction::new()
                                .child(Button::new("confirm-discard-files").danger().label("确认")),
                        ),
                )
                .on_ok(move |_, w, cx| {
                    let _ = confirm.update(cx, |this, cx| {
                        if this.prompt_matches(&confirm_token, &confirm_session) {
                            this.document_state.prompt = None;
                            this.document_state.discard_permit = true;
                            this.perform_document_leave(action.clone(), w, cx);
                            this.document_state.discard_permit = false;
                        }
                    });
                    true
                })
                .on_close(move |_, w, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.clear_document_prompt(&close_token, w, cx)
                    });
                })
        });
        false
    }

    fn prompt_matches(&self, token: &str, session: &Option<String>) -> bool {
        self.document_state.prompt.as_deref() == Some(token)
            && self.session().map(|s| s.id.clone()) == *session
    }

    fn clear_document_prompt(&mut self, token: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_state.prompt.as_deref() == Some(token) {
            self.document_state.prompt = None;
            cx.notify();
        }
        self.defer_document_focus(window, cx);
    }

    fn defer_document_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_state.tabs.is_empty() {
            return;
        }
        cx.defer_in(window, |this, window, cx| {
            if !window.has_active_dialog(cx) {
                this.focus_document_editor(window, cx);
            }
        });
    }

    fn perform_document_leave(
        &mut self,
        action: LeaveAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            LeaveAction::Activate(id) => {
                if let Some(ix) = self.db.sessions.iter().position(|s| s.id == id) {
                    self.activate(ix, window, cx);
                }
            }
            LeaveAction::NewSession(path) => self.new_project_session(path, window, cx),
            LeaveAction::ManageSession { id, delete } => {
                self.manage_session(&id, delete, window, cx)
            }
            LeaveAction::CloseAll => {
                self.clear_documents();
                cx.notify();
            }
            LeaveAction::CloseWindow => {
                self.clear_documents();
                window.remove_window();
            }
        }
    }

    pub(in crate::desktop::workspace) fn request_close_all_documents(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.guard_documents(LeaveAction::CloseAll, window, cx) {
            self.clear_documents();
            cx.notify();
        }
    }

    pub(in crate::desktop::workspace) fn should_close_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.guard_documents(LeaveAction::CloseWindow, window, cx)
    }
}

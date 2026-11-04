//! Reveal event boundaries and deliver attention through the Kit's OS notifications.
use super::*;
use gpui_kit::component::{
    WindowExt,
    notification::{Notification, NotificationDelivery},
};

struct SessionAttention;

impl Workspace {
    pub(in crate::desktop::workspace) fn reveal_pending_tail(&mut self) {
        if !self.polling_background && std::mem::take(&mut self.runtime.reveal_tail) {
            self.runtime.list.scroll_to_end();
        }
    }

    pub(super) fn notify_session_attention(
        &self,
        waiting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.is_window_active() {
            return;
        }
        let Some(session) = self.session() else {
            return;
        };
        let id = session.id.clone();
        let workspace = cx.entity().downgrade();
        window.push_notification(
            Notification::new()
                .id1::<SessionAttention>(SharedString::from(id.clone()))
                .title(if waiting {
                    "会话待选择"
                } else {
                    "会话完成"
                })
                .message(format!(
                    "{} · {}",
                    session.project_location().label(),
                    session.title
                ))
                .delivery(NotificationDelivery::System)
                .on_click(move |_, window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.reveal_notified_session(&id, window, cx);
                    });
                }),
            cx,
        );
    }

    pub(in crate::desktop::workspace) fn reveal_notified_session(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.db.sessions.iter().find(|s| s.id == id && !s.deleted) else {
            return;
        };
        let environment = session.environment_id().to_string();
        if !self.guard_documents(documents::LeaveAction::RevealSession(id.into()), window, cx) {
            return;
        }
        if environment != self.db.active_environment {
            self.switch_environment(&environment, window, cx);
        }
        if self.db.active_environment == environment
            && let Some(ix) = self.db.sessions.iter().position(|s| s.id == id)
        {
            self.activate(ix, window, cx);
            if self.session().is_some_and(|s| s.id == id) {
                self.runtime.list.scroll_to_end();
                cx.notify();
            }
        }
    }
}

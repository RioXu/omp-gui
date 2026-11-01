//! Keep inactive conversations alive and dispatch each event against its owner.
use super::*;

impl SessionRuntime {
    pub(in crate::desktop::workspace) fn release_idle_connection(&mut self) {
        if let Some(connection) = self.engine.take() {
            let (tx, rx) = mpsc::channel();
            self.releasing = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(
                    connection
                        .release_idle()
                        .map_err(|error| format!("{error:#}")),
                );
            });
        }
        self.connected = false;
    }
}

impl Workspace {
    pub(in crate::desktop::workspace) fn session_is_running(&self, id: &str) -> bool {
        if self.session().is_some_and(|s| s.id == id) {
            self.runtime.running
                || self.runtime.pending_send.is_some()
                || self.runtime.preparing.is_some()
        } else {
            self.background
                .get(id)
                .is_some_and(|r| r.running || r.pending_send.is_some() || r.preparing.is_some())
        }
    }

    pub(in crate::desktop::workspace) fn switch_session_runtime(
        &mut self,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.background.remove(target).unwrap_or_else(|| {
            let draft = self
                .db
                .sessions
                .iter()
                .find(|s| s.id == target)
                .map(|s| s.draft.clone())
                .unwrap_or_default();
            Self::new_session_runtime(window, cx, self.demo, draft, None)
        });
        let mut previous = std::mem::replace(&mut self.runtime, next);
        if !previous.running
            && !previous.reconnecting
            && previous.preparing.is_none()
            && !previous.starting
            && previous.pending_send.is_none()
            && previous.requests.is_empty()
        {
            previous.release_idle_connection();
        }
        if let Some(session) = self.session() {
            self.background.insert(session.id.clone(), previous);
        }
    }

    pub(in crate::desktop::workspace) fn poll_background_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible = self.active;
        let ids = self.background.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.dispatch_background(&id, window, cx, |this, window, cx| {
                if this.poll_engine(window, cx) {
                    cx.notify();
                }
                if !this.runtime.running
                    && !this.runtime.reconnecting
                    && this.runtime.preparing.is_none()
                    && !this.runtime.starting
                    && this.runtime.pending_send.is_none()
                    && this.runtime.requests.is_empty()
                {
                    this.runtime.release_idle_connection();
                }
            });
        }
        debug_assert_eq!(self.active, visible);
    }

    fn dispatch_background(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        dispatch: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) {
        let Some(mut state) = self.background.remove(id) else {
            return;
        };
        let Some(ix) = self.db.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        let visible = self.active;
        // Swapping is synchronous: no UI rendering or deferred callback can
        // observe a different active session in the middle of this dispatch.
        std::mem::swap(&mut self.runtime, &mut state);
        self.active = Some(ix);
        self.polling_background = true;
        dispatch(self, window, cx);
        self.polling_background = false;
        self.active = visible;
        std::mem::swap(&mut self.runtime, &mut state);
        self.background.insert(id.to_owned(), state);
    }
}

#[cfg(test)]
pub(in crate::desktop::workspace) mod tests;

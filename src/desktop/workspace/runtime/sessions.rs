//! Keep inactive conversations alive and dispatch each event against its owner.
use super::*;

impl SessionRuntime {
    fn busy(&self) -> bool {
        self.running
            || self.reconnecting
            || self.preparing.is_some()
            || self.starting
            || self.pending_send.is_some()
            || !self.requests.is_empty()
            || self.connection_result.is_some()
            || self.stopping
            || self.collaboration.loading
    }
    fn needs_poll(&self) -> bool {
        self.busy() || self.engine.is_some()
    }
    fn cache_bytes(&self) -> usize {
        self.texts
            .values()
            .map(|entry| entry.body.len().saturating_mul(4))
            .sum()
    }
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
        previous.cached_at = Instant::now();
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
        let ids = self
            .background
            .iter()
            .filter(|(_, state)| state.needs_poll())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
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
        self.trim_session_cache();
        debug_assert_eq!(self.active, visible);
    }

    fn trim_session_cache(&mut self) {
        // Release receipts stay alive until the native writer has exited, even
        // under memory pressure, so fast reconnect/delete cannot race it.
        for state in self.background.values_mut() {
            if let Some(rx) = &state.releasing {
                match rx.try_recv() {
                    Ok(Ok(())) | Err(mpsc::TryRecvError::Disconnected) => state.releasing = None,
                    Ok(Err(error)) => {
                        state.error = Some(error);
                        state.releasing = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        let mut idle = self
            .background
            .iter()
            .filter(|(_, state)| {
                !state.needs_poll()
                    && state.releasing.is_none()
                    && state.follow_up.is_none()
                    && state.steering_request.is_none()
            })
            .map(|(id, state)| (id.clone(), state.cached_at, state.cache_bytes()))
            .collect::<Vec<_>>();
        idle.sort_by_key(|(_, at, _)| *at);
        let mut bytes: usize = idle.iter().map(|(_, _, bytes)| bytes).sum();
        let mut count = idle.len();
        for (id, at, size) in idle {
            if count <= 3 && bytes <= 32 * 1024 * 1024 && at.elapsed() < Duration::from_secs(120) {
                break;
            }
            self.background.remove(&id);
            count -= 1;
            bytes = bytes.saturating_sub(size);
        }
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

#[cfg(test)]
mod performance_tests;

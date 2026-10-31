use super::*;

impl Workspace {
    pub(in crate::desktop::workspace) fn poll_engine(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.poll_collaboration(cx);
        if self.runtime.starting
            && self.runtime.requests.is_empty()
            && self.runtime.last_state.elapsed() > Duration::from_secs(60)
        {
            self.runtime.engine = None;
            self.runtime.starting = false;
            self.runtime.error =
                Some("OMP 连接超时，请检查路径或在终端启动 OMP 查看初始化问题".into());
            cx.notify();
        }
        let events = self
            .runtime
            .engine
            .as_ref()
            .map(|e| e.events.try_iter().take(200).collect::<Vec<_>>())
            .unwrap_or_default();
        let changed = !events.is_empty();
        for event in events {
            match event {
                Event::Frame(f) => self.on_frame(f, window, cx),
                Event::Error(e) => {
                    self.runtime.error = Some(e);
                }
                Event::Closed => {
                    self.runtime.engine = None;
                    self.runtime.active_tools.clear();
                    if let Some(pending) = self.runtime.pending_send.take() {
                        let draft = self
                            .runtime
                            .composer_state
                            .input
                            .read(cx)
                            .value()
                            .to_string();
                        let recovered = if draft.is_empty() {
                            pending.text
                        } else {
                            format!("{}\n\n{draft}", pending.text)
                        };
                        self.runtime.composer_state.input.update(cx, |input, cx| {
                            input.set_value(recovered.clone(), window, cx)
                        });
                        if let Some(s) = self.session_mut() {
                            s.draft = recovered;
                            crate::domain::attachments::merge_attachments(
                                &mut s.draft_attachments,
                                pending.attachments,
                            );
                        }
                        self.runtime.follow_up = pending.follow_up;
                    }
                    self.runtime.connected = false;
                    self.runtime.starting = false;
                    self.runtime.running = false;
                    self.runtime.stopping = false;
                    if let Some(session) = self.session_mut() {
                        session.recover_unconfirmed_queue();
                        let draft = session.draft.clone();
                        self.runtime
                            .composer_state
                            .input
                            .update(cx, |input, cx| input.set_value(draft, window, cx));
                        self.runtime.text_update_pending = true;
                    }
                    self.runtime.status = "OMP 已断开，可重新连接".into();
                    self.dirty = true;
                }
            }
        }
        if self.runtime.text_update_pending {
            self.sync_texts(cx);
            self.runtime.text_update_pending = false;
        }
        if self.runtime.connected
            && self.runtime.running
            && self.runtime.last_state.elapsed() > Duration::from_secs(3)
        {
            self.request_state();
            self.command(json!({"type":"get_subagents","id":"agents"}));
        }
        changed
    }

    pub fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.poll_sessions(window, cx);
        self.poll_attachments(cx);
        self.poll_background_sessions(window, cx);
        let mut changed = self.poll_engine(window, cx);
        // Markdown parsing can finish after the stream update that scheduled it.
        if self.search_state.open
            && (self.search_state.scope != self.document_state.active
                || (self.document_state.active.is_none()
                    && self.runtime.texts.iter().any(|(id, entry)| {
                        self.search_state.rendered_text.get(id)
                            != Some(&entry.state.read(cx).rendered_text())
                    }))
                || self
                    .document_state
                    .tabs
                    .iter()
                    .find(|t| Some(&t.key) == self.document_state.active.as_ref())
                    .and_then(|t| t.entry.as_ref())
                    .is_some_and(|entry| entry.body != self.search_state.source))
        {
            self.update_search(false, cx);
        }
        let file_events = self.document_state.rx.try_iter().collect::<Vec<_>>();
        changed |= !file_events.is_empty();
        for event in file_events {
            self.on_file(event, window, cx);
        }

        self.watch_files();
        if self
            .document_state
            .refresh_pending
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(250))
            || self.document_state.last_scan.elapsed()
                > Duration::from_secs(if self.document_state.watcher.is_some() {
                    30
                } else {
                    5
                })
        {
            self.refresh_files(cx);
        }

        for e in self.persistence.errors.try_iter() {
            self.runtime.error = Some(e);
            changed = true;
        }
        if self.selection_dirty && self.settings_view.is_none() && self.comment_editor.is_none() {
            self.sync_selected_editor_snapshot(cx);
            self.selection_dirty = false;
            let mut selected = None;
            let active_ids = self
                .session()
                .map(|s| {
                    s.messages
                        .iter()
                        .filter(|m| m.role != "user")
                        .map(|m| m.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let entries = active_ids
                .iter()
                .filter_map(|id| self.runtime.texts.get(id))
                .filter(|_| self.document_state.active.is_none())
                .chain(
                    self.document_state
                        .text
                        .as_ref()
                        .filter(|_| self.document_state.active.is_some()),
                );
            for entry in entries {
                let state = entry.state.read(cx);
                if !state.is_selecting() {
                    let quote = state.selected_text();
                    if !quote.is_empty()
                        && let Some(range) = state.selected_source_range()
                    {
                        selected =
                            Anchor::new(entry.source.clone(), entry.body.clone(), range, quote)
                                .ok();
                    }
                }
            }
            if let Some(tab) = self
                .document_state
                .tabs
                .iter()
                .find(|t| Some(&t.key) == self.document_state.active.as_ref())
                && let Some(editor) = &tab.editor
                && tab.diff.is_none()
                && let Some(entry) = &tab.entry
            {
                let editor = editor.read(cx);
                let range = editor.selected_range();
                if !range.is_empty() {
                    let offset = entry.body.find('\n').map(|i| i + 1).unwrap_or(0);
                    selected = Anchor::new(
                        entry.source.clone(),
                        entry.body.clone(),
                        (range.start + offset)..(range.end + offset),
                        editor.selected_text().to_string(),
                    )
                    .ok();
                }
            }
            if self.document_state.active.is_some()
                && let Some(diff) = &self.document_state.diff
            {
                let quote = diff.read(cx).selected_text(cx);
                if !quote.trim().is_empty()
                    && let Some(entry) = &self.document_state.text
                {
                    selected = Anchor::from_quote(entry.source.clone(), entry.body.clone(), quote)
                        .ok()
                        .map(|mut a| {
                            a.code_lines = diff.read(cx).selected_lines().map(|r| CodeLines {
                                original: r.side() == DiffSide::Original,
                                end_original: r.end_side() == DiffSide::Original,
                                first: r.start(),
                                last: r.end(),
                            });
                            a
                        });
                }
            }
            if self
                .selected
                .as_ref()
                .map(|s| (&s.source, &s.range, &s.digest, &s.quote))
                != selected
                    .as_ref()
                    .map(|s| (&s.source, &s.range, &s.digest, &s.quote))
            {
                self.selected = selected;
                self.selection_point = window.mouse_position();
                changed = true;
            }
        }
        if self.dirty && self.last_save.elapsed() > Duration::from_millis(750) {
            self.persist();
        }
        if changed {
            cx.notify();
        }
    }
}

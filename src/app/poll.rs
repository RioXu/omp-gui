use super::*;

impl Workspace {
    pub fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.starting
            && self.requests.is_empty()
            && self.last_state.elapsed() > Duration::from_secs(60)
        {
            self.engine = None;
            self.starting = false;
            self.error = Some("OMP 连接超时，请检查路径或在终端启动 OMP 查看初始化问题".into());
            cx.notify();
        }
        let events = self
            .engine
            .as_ref()
            .map(|e| e.events.try_iter().take(200).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut changed = !events.is_empty();
        for event in events {
            match event {
                Event::Frame(f) => self.on_frame(f, window, cx),
                Event::Error(e) => {
                    self.error = Some(e);
                }
                Event::Closed => {
                    self.connected = false;
                    self.starting = false;
                    self.running = false;
                    self.status = "OMP 已断开，可重新连接".into();
                }
            }
        }
        if self.text_update_pending {
            self.sync_texts(cx);
            self.text_update_pending = false;
        }
        let file_events = self.file_rx.try_iter().collect::<Vec<_>>();
        changed |= !file_events.is_empty();
        for event in file_events {
            self.on_file(event, cx);
        }

        for e in self.persistence.errors.try_iter() {
            self.error = Some(e);
            changed = true;
        }
        if self.connected && self.running && self.last_state.elapsed() > Duration::from_secs(3) {
            self.request_state();
            self.command(json!({"type":"get_subagents","id":"agents"}));
        }
        if self.selection_dirty && self.settings_view.is_none() && self.comment_editor.is_none() {
            self.selection_dirty = false;
            let mut selected = None;
            let active_ids = self
                .session()
                .map(|s| {
                    s.messages
                        .iter()
                        .filter(|m| m.role == "assistant")
                        .map(|m| m.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let entries = active_ids
                .iter()
                .filter_map(|id| self.texts.get(id))
                .chain(self.panel_text.as_ref().filter(|_| self.panel_open));
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
            if self.panel_open
                && let Some(diff) = &self.diff
            {
                let quote = diff.read(cx).selected_text(cx);
                if !quote.trim().is_empty()
                    && let Some(entry) = &self.panel_text
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

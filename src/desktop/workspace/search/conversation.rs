use super::*;
use gpui_kit::base::input::SearchMatcher;

impl Workspace {
    pub fn open_search(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.search_state.open = true;
        self.search_state.input.focus_handle(cx).focus(w, cx);
        self.search_state
            .input
            .update(cx, |i, cx| i.select_all(w, cx));
        self.update_search(true, cx);
    }

    pub fn close_search(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.search_state.open = false;
        self.search_state.hits.clear();
        self.search_state.rendered_text.clear();
        self.clear_search_marks(cx);
        if let Some(tab) = self
            .document_state
            .tabs
            .iter()
            .find(|t| Some(&t.key) == self.document_state.active.as_ref())
        {
            if let Some(diff) = &tab.diff {
                diff.focus_handle(cx).focus(w, cx);
            } else if let Some(editor) = &tab.editor {
                editor.focus_handle(cx).focus(w, cx);
            }
        } else {
            self.runtime
                .composer_state
                .input
                .focus_handle(cx)
                .focus(w, cx);
        }
        cx.notify();
    }

    pub fn update_search(&mut self, reveal: bool, cx: &mut Context<Self>) {
        if self.search_state.scope != self.document_state.active {
            self.clear_search_marks(cx);
            self.search_state.index = 0;
            self.search_state.scope = self.document_state.active.clone();
        }
        if self.document_state.active.is_some() && self.active_subagent().is_none() {
            self.update_document_search(reveal, cx);
            return;
        }
        let previous = self.search_state.hits.get(self.search_state.index).cloned();
        self.search_state.hits.clear();
        self.sync_thinking_search(cx);
        if reveal {
            let pending: HashMap<_, _> = self
                .search_state
                .thinking
                .iter()
                .filter_map(|(key, entry)| {
                    let (display, _) = crate::desktop::markdown::prepare(&entry.body);
                    (entry.state.read(cx).rendered_text().source() != display.as_ref())
                        .then(|| (key.clone(), display.to_string()))
                })
                .collect();
            self.search_state.pending_thinking_reveal = (!pending.is_empty()).then_some(pending);
        }
        let views = self.conversation_search_views();
        self.search_state.rendered_text = views
            .iter()
            .map(|(id, state)| (id.clone(), state.read(cx).rendered_text()))
            .collect();
        let query = self.search_state.input.read(cx).value();
        if self.search_state.open && !query.is_empty() {
            let mut matcher = SearchMatcher::new();
            matcher.update_query(&query, true);
            for (id, state) in views {
                matcher.update(&ropey::Rope::from_str(
                    state.read(cx).rendered_text().as_str(),
                ));
                self.search_state.hits.extend(
                    matcher
                        .matched_ranges()
                        .iter()
                        .cloned()
                        .map(|range| (id.clone(), range)),
                );
            }
        }
        if !reveal
            && let Some(previous) = previous
            && let Some(ix) = self.search_state.hits.iter().position(|h| h == &previous)
        {
            self.search_state.index = ix;
        }
        self.search_state.index = self
            .search_state
            .index
            .min(self.search_state.hits.len().saturating_sub(1));
        self.paint_search(cx);
        // A new match from streaming output must not move the reader's viewport.
        let stale = self
            .search_state
            .pending_thinking_reveal
            .as_ref()
            .is_some_and(|pending| {
                pending.iter().any(|(key, text)| {
                    self.search_state.thinking.get(key).is_none_or(|entry| {
                        crate::desktop::markdown::prepare(&entry.body).0.as_ref() != text
                    })
                })
            });
        if stale {
            self.search_state.pending_thinking_reveal = None;
        }
        let ready =
            self.search_state
                .pending_thinking_reveal
                .as_ref()
                .is_some_and(|pending| {
                    pending.iter().all(|(key, text)| {
                        self.search_state.thinking.get(key).is_some_and(|entry| {
                            entry.state.read(cx).rendered_text().source() == text
                        })
                    })
                });
        let should_reveal =
            ready || (reveal && self.search_state.pending_thinking_reveal.is_none());
        if ready {
            self.search_state.pending_thinking_reveal = None;
        }
        if should_reveal {
            self.reveal_search(cx);
        }
        cx.notify();
    }

    fn paint_search(&self, cx: &mut Context<Self>) {
        let color = cx.theme().warning.opacity(0.28);
        let active_color = cx.theme().selection;
        let mut highlights: HashMap<&str, Vec<RangeHighlight>> = HashMap::new();
        for (ix, (id, range)) in self.search_state.hits.iter().enumerate() {
            highlights
                .entry(id.as_str())
                .or_default()
                .push(RangeHighlight::new(
                    range.clone(),
                    if ix == self.search_state.index {
                        active_color
                    } else {
                        color
                    },
                ));
        }
        for (id, state) in self.conversation_search_views() {
            let highlights = highlights.remove(id.as_str()).unwrap_or_default();
            state.update(cx, |s, cx| {
                let _ = s.set_range_highlights(highlights, cx);
            });
        }
    }

    pub fn move_search(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.document_state.active.is_some() && self.active_subagent().is_none() {
            self.move_document_search(backwards, cx);
            return;
        }
        let count = self.search_state.hits.len();
        if count == 0 {
            return;
        }
        self.search_state.index =
            (self.search_state.index + if backwards { count - 1 } else { 1 }) % count;
        self.paint_search(cx);
        self.reveal_search(cx);
        cx.notify();
    }

    fn reveal_search(&mut self, cx: &mut Context<Self>) {
        let Some((id, range)) = self.search_state.hits.get(self.search_state.index).cloned() else {
            return;
        };
        let thinking = self.search_state.thinking.get(&id);
        let message_id = thinking
            .map_or(id.as_str(), |entry| entry.message.as_str())
            .to_owned();
        let target = self
            .conversation_search_views()
            .into_iter()
            .find(|(key, _)| key == &id)
            .map(|(_, state)| state);
        if thinking.is_some() {
            self.runtime.expanded_process.insert(id.clone());
        }
        if self.active_subagent().is_some() {
            let state = &self.runtime.collaboration;
            let Some(ix) = state.messages.iter().position(|m| m.id == message_id) else {
                return;
            };
            let Some(row) = state.rows.iter().position(|r| r.contains(ix)) else {
                return;
            };
            if let TranscriptRow::Assistant(group) = &state.rows[row] {
                self.runtime.expanded_process.insert(format!(
                    "child-turn-{}-{}",
                    state.selected.as_deref().unwrap_or(""),
                    state.messages[group.start].id
                ));
            }
            self.invalidate_list();
            self.runtime.collaboration.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        } else {
            let Some(ix) = self
                .session()
                .and_then(|s| s.messages.iter().position(|m| m.id == message_id))
            else {
                return;
            };
            self.ensure_message_visible(ix, cx);
            let row = self.row_for_message(ix);
            if let Some(TranscriptRow::Notice(_)) = self.runtime.transcript.get(row) {
                self.runtime
                    .expanded_process
                    .insert(format!("notice-{message_id}"));
            }
            if let Some(TranscriptRow::Assistant(group)) = self.runtime.transcript.get(row)
                && let Some(s) = self.session()
            {
                self.runtime
                    .expanded_process
                    .insert(format!("turn-{}", s.messages[group.start].id));
            }
            self.invalidate_list();
            self.runtime.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
        if let Some(state) = target {
            state.update(cx, |s, cx| {
                let _ = s.reveal_range(range, cx);
            });
        }
    }

    fn sync_thinking_search(&mut self, cx: &mut Context<Self>) {
        if !self.search_state.open {
            self.search_state.thinking.clear();
            return;
        }
        let child = self.active_subagent().is_some();
        let messages = if child {
            self.runtime.collaboration.messages.as_slice()
        } else {
            self.session().map_or(&[][..], |s| s.messages.as_slice())
        };
        let mut sources = Vec::new();
        for message in messages {
            let visible = if child {
                self.runtime.collaboration.texts.contains_key(&message.id)
            } else {
                self.runtime.texts.contains_key(&message.id)
            };
            if !visible || message.state == MessageState::Queued {
                continue;
            }
            for block in &message.process {
                if block.kind == ProcessKind::Thinking {
                    let key = format!(
                        "process-{}{}-{}",
                        if child { "child-" } else { "" },
                        message.id,
                        block.id
                    );
                    sources.push((key, message.id.clone(), block.text.clone()));
                }
            }
        }
        self.search_state
            .thinking
            .retain(|key, _| sources.iter().any(|(id, _, _)| id == key));
        for (key, message, body) in sources {
            if let Some(entry) = self.search_state.thinking.get_mut(&key) {
                if entry.body != body {
                    let (display, _) = crate::desktop::markdown::prepare(&body);
                    entry
                        .state
                        .update(cx, |s, cx| s.set_text(display.as_ref(), cx));
                    entry.body = body;
                }
            } else {
                let (display, _) = crate::desktop::markdown::prepare(&body);
                let state = cx.new(|cx| TextViewState::markdown(&display, cx));
                self.search_state.thinking.insert(
                    key,
                    super::ThinkingSearch {
                        message,
                        body,
                        state,
                    },
                );
            }
        }
    }

    pub(in crate::desktop::workspace) fn conversation_search_views(
        &self,
    ) -> Vec<(String, Entity<TextViewState>)> {
        let child = self.active_subagent().is_some();
        let messages = if child {
            self.runtime.collaboration.messages.as_slice()
        } else {
            self.session().map_or(&[][..], |s| s.messages.as_slice())
        };
        let mut views = Vec::new();
        for message in messages {
            let visible = if child {
                self.runtime.collaboration.texts.contains_key(&message.id)
            } else {
                self.runtime.texts.contains_key(&message.id)
            };
            if !visible || message.state == MessageState::Queued {
                continue;
            }
            for block in &message.process {
                let key = format!(
                    "process-{}{}-{}",
                    if child { "child-" } else { "" },
                    message.id,
                    block.id
                );
                if let Some(entry) = self.search_state.thinking.get(&key) {
                    views.push((key, entry.state.clone()));
                }
            }
            let state = if child {
                self.runtime
                    .collaboration
                    .texts
                    .get(&message.id)
                    .map(|(_, view)| view)
            } else {
                self.runtime
                    .texts
                    .get(&message.id)
                    .map(|entry| &entry.state)
            };
            if let Some(state) = state {
                views.push((message.id.clone(), state.clone()));
            }
        }
        views
    }
}

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
        if self.document_state.active.is_some() {
            self.update_document_search(reveal, cx);
            return;
        }
        let previous = self.search_state.hits.get(self.search_state.index).cloned();
        let first_match = previous.is_none();
        self.search_state.hits.clear();
        self.search_state.rendered_text = self
            .runtime
            .texts
            .iter()
            .map(|(id, entry)| (id.clone(), entry.state.read(cx).rendered_text()))
            .collect();
        let query = self.search_state.input.read(cx).value();
        if self.search_state.open && !query.is_empty() {
            let mut matcher = SearchMatcher::new();
            matcher.update_query(&query, true);
            if let Some(session) = self.session() {
                let mut hits = Vec::new();
                for message in &session.messages {
                    if message.state == MessageState::Queued {
                        continue;
                    }
                    if let Some(entry) = self.runtime.texts.get(&message.id) {
                        let rendered = entry.state.read(cx).rendered_text();
                        matcher.update(&ropey::Rope::from_str(rendered.as_str()));
                        hits.extend(
                            matcher
                                .matched_ranges()
                                .iter()
                                .cloned()
                                .map(|r| (message.id.clone(), r)),
                        );
                    }
                }
                self.search_state.hits = hits;
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
        if reveal || (first_match && !self.search_state.hits.is_empty()) {
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
        for (id, entry) in &self.runtime.texts {
            let highlights = highlights.remove(id.as_str()).unwrap_or_default();
            entry.state.update(cx, |s, cx| {
                let _ = s.set_range_highlights(highlights, cx);
            });
        }
    }

    pub fn move_search(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.document_state.active.is_some() {
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
        let Some(ix) = self
            .session()
            .and_then(|s| s.messages.iter().position(|m| m.id == id))
        else {
            return;
        };
        let row = self.row_for_message(ix);
        if let Some(TranscriptRow::Assistant(group)) = self.runtime.transcript.get(row)
            && let Some(s) = self.session()
        {
            self.runtime
                .expanded_process
                .insert(format!("turn-{}", s.messages[group.start].id));
        }
        self.invalidate_list();
        // Invalidated virtual rows have no measured height yet. Reveal-by-height
        // can resolve to the old viewport, so first anchor the target row itself.
        self.runtime.list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
        if let Some(entry) = self.runtime.texts.get(&id) {
            entry.state.update(cx, |s, cx| {
                let _ = s.reveal_range(range, cx);
            });
        }
    }
}

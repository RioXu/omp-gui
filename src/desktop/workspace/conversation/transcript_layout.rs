use super::*;

pub(in crate::desktop::workspace) const HISTORY_PAGE_SIZE: usize = 100;

impl Workspace {
    fn preserve_selected_reading(&self) {
        if self
            .selected
            .as_ref()
            .is_some_and(|a| matches!(a.source, Source::Message(_)))
            && self.runtime.list.is_following_tail()
        {
            let pixels = -self.runtime.list.scroll_px_offset_for_scrollbar().y;
            self.runtime.list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            self.runtime.list.scroll_by(pixels);
            self.runtime.list.pause_following_tail();
        }
    }
    /// A backend transition must not collapse the process the user is reading.
    pub(in crate::desktop::workspace) fn retain_process_display(&mut self) {
        if !self.runtime.running {
            return;
        }
        let Some(TranscriptRow::Assistant(range)) = self.runtime.transcript.last() else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let Some(messages) = session
            .messages
            .get(range.clone())
            .filter(|m| !m.is_empty())
        else {
            return;
        };
        if !messages.iter().any(|m| !m.process.is_empty()) {
            return;
        }
        let key = format!("turn-{}", messages[0].id);
        let previews: Vec<_> = messages
            .iter()
            .flat_map(|m| {
                m.process
                    .iter()
                    .filter(|b| b.kind == ProcessKind::Thinking && b.streaming)
                    .map(|b| format!("process-{}-{}", m.id, b.id))
            })
            .collect();
        self.runtime.expanded_process.insert(key);
        self.runtime.thinking_previews.extend(previews);
    }

    /// Layout changes must not clear the reader's scroll anchor.
    pub fn invalidate_list(&self) {
        invalidate_preserving_scroll(&self.runtime.list);
        invalidate_preserving_scroll(&self.runtime.collaboration.list);
        for list in self.runtime.process_lists.borrow().values() {
            invalidate_preserving_scroll(list);
        }
    }
    pub(in crate::desktop::workspace) fn refresh_transcript(&self) {
        update_list_preserving_scroll(&self.runtime.list, 0, self.runtime.transcript.len());
    }
    pub fn row_for_message(&self, ix: usize) -> usize {
        let row = self.runtime.transcript.partition_point(|row| match row {
            TranscriptRow::User(at) | TranscriptRow::Notice(at) => *at < ix,
            TranscriptRow::Assistant(range) => range.end <= ix,
        });
        self.runtime
            .transcript
            .get(row)
            .filter(|row| row.contains(ix))
            .map_or(0, |_| row)
    }
    pub fn prepare_tool_diff(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.runtime.tool_diffs.contains_key(key) {
            return;
        }
        let patch = self.session().and_then(|s| {
            s.messages
                .iter()
                .chain(self.runtime.collaboration.messages.iter())
                .find_map(|m| {
                    m.process.iter().find_map(|b| {
                        (format!("process-{}-{}", m.id, b.id) == key
                            || format!("process-child-{}-{}", m.id, b.id) == key)
                            .then(|| b.patch.clone())
                            .flatten()
                    })
                })
        });
        if let Some(patch) = patch
            && let Ok(files) = DiffFile::parse(&patch)
            && !files.is_empty()
        {
            self.runtime
                .tool_diffs
                .insert(key.into(), cx.new(|cx| DiffState::new(files, cx)));
        }
    }
}
fn invalidate_preserving_scroll(list: &ListState) {
    list.remeasure_items(0..list.item_count());
}

fn update_list_preserving_scroll(list: &ListState, first: usize, count: usize) {
    let old_count = list.item_count();
    let common = old_count.min(count);
    // Existing rows keep their size hints and absolute reading anchors while
    // Markdown is being parsed. Splice only genuinely added/removed rows.
    list.remeasure_items(first.min(common)..common);
    if old_count != count {
        list.splice(common..old_count, count - common);
    }
}

// Rebuild the affected turn and following rows; preceding history keeps its
// measured heights and scroll anchors. Include the previous row for appends,
// because adjacent assistant messages belong to the same turn.
fn update_transcript_tail(
    rows: &mut Vec<TranscriptRow>,
    messages: &[Message],
    first: usize,
) -> usize {
    let end = |row: &TranscriptRow| match row {
        TranscriptRow::User(ix) | TranscriptRow::Notice(ix) => ix + 1,
        TranscriptRow::Assistant(range) => range.end,
    };
    let row = rows
        .partition_point(|row| end(row) <= first)
        .min(rows.len())
        .saturating_sub(1);
    let start = rows
        .get(row)
        .map_or(first, |row| match row {
            TranscriptRow::User(ix) | TranscriptRow::Notice(ix) => *ix,
            TranscriptRow::Assistant(range) => range.start,
        })
        .min(first)
        .min(messages.len());
    rows.truncate(row);
    rows.extend(
        transcript_rows(&messages[start..])
            .into_iter()
            .map(|row| match row {
                TranscriptRow::User(ix) => TranscriptRow::User(ix + start),
                TranscriptRow::Notice(ix) => TranscriptRow::Notice(ix + start),
                TranscriptRow::Assistant(range) => {
                    TranscriptRow::Assistant(range.start + start..range.end + start)
                }
            }),
    );
    row
}

impl Workspace {
    pub(in crate::desktop::workspace) fn message_index(&self, id: &str) -> usize {
        let Some(session) = self.session() else {
            return 0;
        };
        self.runtime
            .message_indices
            .get(id)
            .copied()
            .filter(|ix| session.messages.get(*ix).is_some_and(|m| m.id == id))
            .or_else(|| session.messages.iter().position(|m| m.id == id))
            .unwrap_or(session.messages.len())
    }
    pub fn sync_texts(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        self.preserve_selected_reading();
        let following = self.runtime.list.is_following_tail();
        let offset = self.runtime.list.logical_scroll_top();
        let anchor = self
            .runtime
            .transcript
            .get(offset.item_ix)
            .map(|row| match row {
                TranscriptRow::User(ix) | TranscriptRow::Notice(ix) => *ix,
                TranscriptRow::Assistant(range) => range.start,
            });
        let end = self
            .runtime
            .history_end
            .unwrap_or(self.db.sessions[ix].messages.len())
            .min(self.db.sessions[ix].messages.len());
        let start = end.saturating_sub(HISTORY_PAGE_SIZE);
        self.runtime.history_start = start;
        self.runtime.message_indices = self.db.sessions[ix]
            .messages
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .map(|(ix, m)| (m.id.clone(), ix))
            .collect();
        self.runtime
            .texts
            .retain(|id, _| self.runtime.message_indices.contains_key(id));
        self.runtime.transcript.clear();
        self.runtime.transcript_message_count = 0;
        self.runtime.text_updates = (start..end).collect();
        if self.runtime.text_updates.is_empty() {
            self.refresh_transcript();
        } else {
            self.sync_updated_texts(cx);
        }
        if !following && let Some(index) = anchor.filter(|index| *index >= start && *index < end) {
            self.runtime.list.scroll_to(ListOffset {
                item_ix: self.row_for_message(index),
                offset_in_item: offset.offset_in_item,
            });
        }
        self.reveal_pending_tail(cx);
    }
    pub(in crate::desktop::workspace) fn sync_updated_texts(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else { return };
        self.preserve_selected_reading();
        let end = self
            .runtime
            .history_end
            .unwrap_or(self.db.sessions[ix].messages.len())
            .min(self.db.sessions[ix].messages.len());
        if self.runtime.history_start != end.saturating_sub(HISTORY_PAGE_SIZE) {
            self.sync_texts(cx);
            return;
        }
        let updates: Vec<_> = std::mem::take(&mut self.runtime.text_updates)
            .into_iter()
            .filter(|ix| *ix >= self.runtime.history_start && *ix < end)
            .collect();
        let Some(&first) = updates.first() else {
            return;
        };
        // Native tail mode follows growing rows only while the reader stays
        // at the bottom; scrolling up retains an absolute reading anchor.
        let messages = &self.db.sessions[ix].messages[..end];
        for index in updates {
            let Some(m) = messages.get(index) else {
                continue;
            };
            let body = omp_core::domain::notice::display_body(m);
            if let Some(entry) = self.runtime.texts.get_mut(&m.id) {
                if entry.body != body {
                    entry.body = body.to_string();
                    let (display, offsets) = crate::desktop::markdown::prepare(&body);
                    entry.offsets = offsets;
                    entry
                        .state
                        .update(cx, |s, cx| s.set_text(display.as_ref(), cx));
                }
            } else {
                let (display, offsets) = crate::desktop::markdown::prepare(&body);
                let state =
                    cx.new(|cx| TextViewState::markdown(&display, cx).selectable(m.role != "user"));
                self.runtime.texts.insert(
                    m.id.clone(),
                    TextEntry {
                        state,
                        body: body.into_owned(),
                        source: Source::Message(m.id.clone()),
                        offsets,
                    },
                );
            }
        }
        let row = if self.runtime.transcript_message_count == messages.len() {
            self.row_for_message(first)
        } else {
            update_transcript_tail(&mut self.runtime.transcript, messages, first)
        };
        self.runtime.transcript_message_count = messages.len();
        update_list_preserving_scroll(&self.runtime.list, row, self.runtime.transcript.len());
        if self.search_state.open && !self.polling_background {
            self.update_search(false, cx);
        }
    }

    pub(in crate::desktop::workspace) fn show_history_page(
        &mut self,
        end: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.runtime.history_end = end;
        self.runtime.list.set_follow_mode(if end.is_none() {
            gpui_kit::FollowMode::Tail
        } else {
            gpui_kit::FollowMode::Normal
        });
        self.runtime.texts.clear();
        self.runtime.tool_diffs.clear();
        self.runtime.process_lists.borrow_mut().clear();
        self.runtime.process_texts.borrow_mut().clear();
        self.sync_texts(cx);
        self.runtime.list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn ensure_message_visible(
        &mut self,
        ix: usize,
        cx: &mut Context<Self>,
    ) {
        let end = self
            .runtime
            .history_end
            .unwrap_or_else(|| self.session().map_or(0, |s| s.messages.len()));
        if ix < self.runtime.history_start || ix >= end {
            self.show_history_page(Some(ix.saturating_add(1).max(HISTORY_PAGE_SIZE)), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::invalidate_preserving_scroll;
    use gpui_kit::{ListAlignment, ListOffset, ListState, px};
    #[test]
    fn incremental_rows_match_full_grouping_after_appends_and_queue_delivery() {
        use super::*;
        let mut messages = Vec::new();
        let mut rows = Vec::new();
        for role in [
            "user",
            "assistant",
            "assistant",
            "user",
            "assistant",
            "custom",
            "assistant",
        ] {
            let first = messages.len();
            messages.push(Message::new(role, String::new()));
            update_transcript_tail(&mut rows, &messages, first);
            assert_eq!(rows, transcript_rows(&messages));
        }
        for index in 0..messages.len() {
            messages[index].state = MessageState::Queued;
            update_transcript_tail(&mut rows, &messages, index);
            assert_eq!(rows, transcript_rows(&messages));
            messages[index].state = MessageState::Normal;
            update_transcript_tail(&mut rows, &messages, index);
            assert_eq!(rows, transcript_rows(&messages));
        }
    }
    #[test]
    fn expanding_history_keeps_logical_anchor_instead_of_jumping_to_tail() {
        let list = ListState::new(100, ListAlignment::Bottom, px(600.));
        list.scroll_to(ListOffset {
            item_ix: 12,
            offset_in_item: px(7.),
        });
        invalidate_preserving_scroll(&list);
        let offset = list.logical_scroll_top();
        assert_eq!(offset.item_ix, 12);
        assert_eq!(offset.offset_in_item, px(7.));
        assert!(!list.is_following_tail());
    }
}

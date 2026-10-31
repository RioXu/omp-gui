use super::*;

impl Workspace {
    /// Layout changes must not clear the reader's scroll anchor.
    pub fn invalidate_list(&self) {
        invalidate_preserving_scroll(&self.runtime.list);
    }
    pub(in crate::desktop::workspace) fn refresh_transcript(&self) {
        let follow = self.runtime.list.is_following_tail() || self.runtime.list.item_count() == 0;
        let offset = self.runtime.list.logical_scroll_top();
        self.runtime.list.splice(
            0..self.runtime.list.item_count(),
            self.runtime.transcript.len(),
        );
        if follow {
            self.runtime.list.scroll_to_end();
        } else {
            self.runtime.list.scroll_to(offset);
        }
    }
    pub fn row_for_message(&self, ix: usize) -> usize {
        self.runtime
            .transcript
            .iter()
            .position(|row| row.contains(ix))
            .unwrap_or(0)
    }
    pub fn prepare_tool_diff(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.runtime.tool_diffs.contains_key(key) {
            return;
        }
        let patch = self.session().and_then(|s| {
            s.messages.iter().find_map(|m| {
                m.process.iter().find_map(|b| {
                    (format!("process-{}-{}", m.id, b.id) == key)
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
    let offset = list.logical_scroll_top();
    let count = list.item_count();
    list.splice(0..count, count);
    list.scroll_to(offset);
}

impl Workspace {
    pub fn sync_texts(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        let messages = &self.db.sessions[ix].messages;
        for m in messages {
            if let Some(entry) = self.runtime.texts.get_mut(&m.id) {
                if entry.body != m.text {
                    entry.body = m.text.clone();
                    entry.state.update(cx, |s, cx| s.set_text(&m.text, cx));
                }
            } else {
                let state =
                    cx.new(|cx| TextViewState::markdown(&m.text, cx).selectable(m.role != "user"));
                self.runtime.texts.insert(
                    m.id.clone(),
                    TextEntry {
                        state,
                        body: m.text.clone(),
                        source: Source::Message(m.id.clone()),
                    },
                );
            }
        }
        self.runtime.transcript = transcript_rows(messages);
        self.refresh_transcript();
        if self.search_state.open && !self.polling_background {
            self.update_search(false, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::invalidate_preserving_scroll;
    use gpui_kit::{ListAlignment, ListOffset, ListState, px};
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

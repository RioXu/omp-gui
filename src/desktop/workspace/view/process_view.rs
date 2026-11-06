use super::*;
use gpui_kit::component::input::Editor;
use std::{cell::RefCell, sync::Arc};

pub(in crate::desktop::workspace) type ProcessLists = RefCell<HashMap<String, ListState>>;
pub(in crate::desktop::workspace) type ProcessTexts =
    RefCell<HashMap<String, (String, Entity<EditorState>)>>;

#[derive(Clone)]
struct ProcessItem {
    message: Arc<str>,
    block: Option<usize>,
}

fn items(messages: &[&Message]) -> Vec<ProcessItem> {
    let mut result = Vec::with_capacity(messages.iter().map(|m| m.process.len() + 1).sum());
    let last = messages.last().map(|m| m.id.as_str());
    for message in messages {
        if message.state == MessageState::Queued {
            continue;
        }
        let id: Arc<str> = message.id.as_str().into();
        for block in 0..message.process.len() {
            result.push(ProcessItem {
                message: id.clone(),
                block: Some(block),
            });
        }
        if !message.text.is_empty()
            && (Some(message.id.as_str()) != last
                || message.process.iter().any(|b| b.kind == ProcessKind::Tool))
        {
            result.push(ProcessItem {
                message: id,
                block: None,
            });
        }
    }
    result
}

impl Workspace {
    pub(super) fn render_turn_process(
        &self,
        key: String,
        messages: &[&Message],
        active: bool,
        child: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = Arc::new(items(messages));
        let expanded = active || self.runtime.expanded_process.contains(&key);
        let count = items.len();
        if count == 0 {
            return div().into_any_element();
        }
        let content = if !expanded {
            div().into_any_element()
        } else if count <= 12 {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(
                    items
                        .iter()
                        .map(|item| self.render_process_item(item, child, window, cx)),
                )
                .into_any_element()
        } else {
            let state = self.process_list(&key, count, active);
            let weak = cx.entity().downgrade();
            let child_list = state.clone();
            div()
                .relative()
                .h(px(360.))
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .debug_selector(|| "process-viewport".into())
                .child(
                    list(state.clone(), move |index, window, cx| {
                        weak.update(cx, |this, cx| {
                            let body = this.render_process_item(&items[index], child, window, cx);
                            super::transcript::virtual_row(body, index, child_list.clone(), 0.)
                        })
                        .unwrap_or_else(|_| div().into_any_element())
                    })
                    .flex_1()
                    .min_h_0(),
                )
                .vertical_scrollbar(&state)
                .into_any_element()
        };
        self.process_section(
            key,
            if active {
                format!("正在处理 · {count} 项过程")
            } else {
                format!("过程记录 · {count} 项")
            },
            active,
            content,
            cx,
        )
    }

    fn process_list(&self, key: &str, count: usize, active: bool) -> ListState {
        let mut cache = self.runtime.process_lists.borrow_mut();
        if !cache.contains_key(key) && cache.len() >= 64 {
            cache.clear();
        }
        let state = cache.entry(key.to_owned()).or_insert_with(|| {
            let state = ListState::new(count, ListAlignment::Top, px(60.));
            state.set_follow_mode(FollowMode::Tail);
            if !active {
                state.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                });
            }
            state
        });
        let old = state.item_count();
        if old != count {
            state.splice(old.min(count)..old, count.saturating_sub(old));
        }
        state.remeasure_items(0..count);
        state.clone()
    }

    fn render_process_item(
        &self,
        item: &ProcessItem,
        child: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = if child {
            self.runtime
                .collaboration
                .messages
                .iter()
                .find(|m| m.id.as_str() == item.message.as_ref())
        } else {
            self.session().and_then(|s| {
                s.messages
                    .iter()
                    .find(|m| m.id.as_str() == item.message.as_ref())
            })
        };
        let Some(message) = message else {
            return div().into_any_element();
        };
        if let Some(block) = item.block.and_then(|ix| message.process.get(ix)) {
            self.render_process_block(
                &if child {
                    format!("child-{}", message.id)
                } else {
                    message.id.clone()
                },
                block,
                window,
                cx,
            )
        } else if child {
            self.runtime
                .collaboration
                .texts
                .get(&message.id)
                .map(|(_, state)| {
                    self.file_link_view(TextView::new(state), cx)
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element())
        } else {
            self.runtime
                .texts
                .get(&message.id)
                .map(|entry| {
                    div()
                        .px_2()
                        .py_2()
                        .child(self.message_file_link_view(
                            TextView::new(&entry.state),
                            &message.id,
                            cx,
                        ))
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element())
        }
    }

    pub(super) fn process_text(
        &self,
        key: String,
        text: &str,
        language: &str,
        tail: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if text.len() <= 8192 && text.lines().take(121).count() <= 120 {
            return TextView::markdown(SharedString::from(key), fenced(text, language))
                .into_any_element();
        }
        let mut cache = self.runtime.process_texts.borrow_mut();
        if !cache.contains_key(&key)
            && (cache.len() >= 24
                || cache.values().map(|(text, _)| text.len()).sum::<usize>() > 8 * 1024 * 1024)
        {
            cache.clear();
        }
        let (source, state) = cache.entry(key.clone()).or_insert_with(|| {
            let state = cx.new(|cx| {
                let mut state = EditorState::new(window, cx)
                    .language("plain")
                    .line_number(true);
                state.set_value(text, window, cx);
                if tail {
                    state.set_scroll_offset(point(px(0.), px(-1e9)), cx);
                }
                state
            });
            (text.to_owned(), state)
        });
        if source != text {
            state.update(cx, |state, cx| {
                let offset = state.scroll_offset();
                let at_end = tail
                    && state.line_height().is_some_and(|height| {
                        -offset.y + px(260.) + height
                            >= height * source.lines().count().max(1) as f32
                    });
                state.set_value(text, window, cx);
                state.set_scroll_offset(
                    if at_end {
                        point(px(0.), px(-1e9))
                    } else {
                        offset
                    },
                    cx,
                );
            });
            *source = text.to_owned();
        }
        div()
            .h(px(260.))
            .w_full()
            .min_w_0()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .overflow_hidden()
            .debug_selector(move || format!("process-text-{key}"))
            .child(
                Editor::new(state)
                    .readonly(true)
                    .appearance(false)
                    .bordered(false)
                    .size_full(),
            )
            .into_any_element()
    }

    pub(in crate::desktop::workspace) fn reveal_process_location(
        &mut self,
        message_id: &str,
        block_key: Option<&str>,
    ) {
        let child = self.active_subagent().is_some();
        let (messages, rows) = if child {
            (
                &self.runtime.collaboration.messages[..],
                &self.runtime.collaboration.rows,
            )
        } else {
            (
                self.session().map_or(&[][..], |s| &s.messages[..]),
                &self.runtime.transcript,
            )
        };
        let Some(ix) = messages.iter().position(|m| m.id == message_id) else {
            return;
        };
        let Some(TranscriptRow::Assistant(group)) = rows.iter().find(|row| row.contains(ix)) else {
            return;
        };
        let key = if child {
            format!(
                "child-turn-{}-{}",
                self.runtime.collaboration.selected.as_deref().unwrap_or(""),
                messages[group.start].id
            )
        } else {
            format!("turn-{}", messages[group.start].id)
        };
        let refs: Vec<_> = messages[group.clone()]
            .iter()
            .filter(|m| m.state != MessageState::Queued)
            .collect();
        let items = items(&refs);
        let at = items.iter().position(|item| {
            if item.message.as_ref() != message_id {
                return false;
            }
            match (block_key, item.block) {
                (Some(key), Some(ix)) => messages
                    .iter()
                    .find(|m| m.id == message_id)
                    .and_then(|m| m.process.get(ix))
                    .is_some_and(|block| {
                        key == format!(
                            "process-{}{}-{}",
                            if child { "child-" } else { "" },
                            message_id,
                            block.id
                        )
                    }),
                (None, None) => true,
                _ => false,
            }
        });
        if let Some(at) = at {
            self.process_list(&key, items.len(), false)
                .scroll_to(ListOffset {
                    item_ix: at,
                    offset_in_item: px(0.),
                });
        }
    }
}

#[cfg(test)]
mod tests;

use super::*;

impl Workspace {
    pub fn begin_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(a) = self.selected.take() {
            self.comment_editor = Some(a);
            self.editing_comment = None;
            self.comment_input
                .update(cx, |i, cx| i.set_value("", window, cx));
            self.comment_input.focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }
    pub fn save_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.comment_input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(anchor) = self.comment_editor.take() else {
            return;
        };
        let editing = self.editing_comment.take();
        if let Some(s) = self.session_mut() {
            if let Some(id) = editing {
                if let Some(c) = s.comments.iter_mut().find(|c| c.id == id && !c.sent) {
                    c.text = text;
                }
            } else {
                s.comments.push(Comment::new(anchor, text));
            }
        }
        self.pending_open = true;
        self.selected = None;
        gpui_kit::base::TextSelection::clear(window, cx);
        self.persist();
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub fn open_anchor(&mut self, anchor: Anchor, cx: &mut Context<Self>) {
        self.diff = None;
        if let Source::Message(id) = &anchor.source
            && let Some((ix, state)) = self
                .session()
                .and_then(|s| s.messages.iter().position(|m| &m.id == id))
                .and_then(|ix| {
                    self.texts
                        .get(id)
                        .filter(|e| e.body == anchor.document)
                        .map(|e| (ix, e.state.clone()))
                })
        {
            let row = self.row_for_message(ix);
            if let Some(TranscriptRow::Assistant(range)) = self.transcript.get(row)
                && let Some(s) = self.session()
            {
                self.expanded_process
                    .insert(format!("turn-{}", s.messages[range.start].id));
            }
            self.invalidate_list();
            let color = cx.theme().selection;
            state.update(cx, |s, cx| {
                if let Some(range) = s.rendered_text().range_for_source(anchor.range.clone()) {
                    let _ = s.set_range_highlights([RangeHighlight::new(range.clone(), color)], cx);
                    let _ = s.reveal_range(range, cx);
                }
            });
            self.list.scroll_to_reveal_item(row);
            self.selected = None;
            cx.notify();
            return;
        }
        self.panel_open = true;
        self.panel_tab = "变更";
        self.historical = true;
        self.viewed_path = match &anchor.source {
            Source::File { path, snapshot } => {
                self.viewed_snapshot = Some(snapshot.clone());
                Some(path.clone())
            }
            _ => {
                self.viewed_snapshot = None;
                None
            }
        };
        self.viewed_quote = Some(anchor.quote.clone());
        if let Source::File { path, snapshot } = &anchor.source
            && let Some(stored) = self.session().and_then(|s| s.snapshots.get(snapshot))
            && let Ok(files) = DiffFile::parse(&stored.patch)
            && !files.is_empty()
        {
            let state = cx.new(|cx| DiffState::new(files, cx));
            if let Some(lines) = &anchor.code_lines {
                let range = DiffLineRange::new(
                    path.to_string_lossy().replace('\\', "/"),
                    if lines.original {
                        DiffSide::Original
                    } else {
                        DiffSide::Modified
                    },
                    lines.first,
                    lines.last,
                )
                .with_end_side(if lines.end_original {
                    DiffSide::Original
                } else {
                    DiffSide::Modified
                });
                state.update(cx, |d, cx| {
                    d.set_selected_lines(Some(range), cx);
                });
            }
            self.diff = Some(state);
        }
        let body = anchor.document.clone();
        let state = cx.new(|cx| TextViewState::markdown(&body, cx).selectable(true));
        let color = cx.theme().selection;
        state.update(cx, |s, cx| {
            if anchor.exact_range
                && let Some(r) = s.rendered_text().range_for_source(anchor.range.clone())
            {
                let _ = s.set_range_highlights([RangeHighlight::new(r.clone(), color)], cx);
                let _ = s.reveal_range(r, cx);
            }
        });
        self.panel_text = Some(TextEntry {
            state,
            body,
            source: anchor.source,
        });
        self.selected = None;
        cx.notify();
    }
    pub fn open_current(&mut self) {
        let Some(path) = self.viewed_path.clone() else {
            return;
        };
        if self.demo {
            self.open_file(path);
            return;
        }
        let Some(s) = self.session() else {
            return;
        };
        let sid = s.id.clone();
        let root = s.cwd().to_owned();
        let tx = self.file_tx.clone();
        std::thread::spawn(move || {
            let result = project_files::read(&root, &path).map_err(|e| format!("{e:#}"));
            let _ = tx.send(FileEvent::Current {
                session: sid,
                path,
                result,
            });
        });
    }
}

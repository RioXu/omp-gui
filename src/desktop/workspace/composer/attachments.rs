//! Import files off the UI thread, routing completion to the original draft.
use super::*;
use crate::infrastructure::filesystem::attachments::{self, Source};
use std::collections::HashSet;

struct Imported {
    session: String,
    job: String,
    result: Result<Attachment, String>,
}

pub(in crate::desktop::workspace) struct AttachmentImports {
    tx: mpsc::Sender<Imported>,
    rx: mpsc::Receiver<Imported>,
    pending: HashMap<String, HashSet<String>>,
}
impl Default for AttachmentImports {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            pending: HashMap::new(),
        }
    }
}

impl Workspace {
    pub(in crate::desktop::workspace) fn importing_attachments(&self) -> bool {
        self.session().is_some_and(|s| {
            self.attachment_imports
                .pending
                .get(&s.id)
                .is_some_and(|jobs| !jobs.is_empty())
        })
    }

    pub(in crate::desktop::workspace) fn import_attachments(
        &mut self,
        sources: Vec<Source>,
        cx: &mut Context<Self>,
    ) {
        if self.save_disabled || self.runtime.preparing.is_some() {
            return;
        }
        let Some(session) = self.session() else {
            self.runtime.error = Some("请先创建或选择会话，再添加附件。".into());
            cx.notify();
            return;
        };
        let sid = session.id.clone();
        let count = session.draft_attachments.len()
            + self
                .attachment_imports
                .pending
                .get(&sid)
                .map_or(0, HashSet::len);
        if count + sources.len() > attachments::MAX_COUNT {
            self.runtime.error = Some("每条消息最多添加 16 个附件".into());
            cx.notify();
            return;
        }
        for source in sources {
            let job = id();
            self.attachment_imports
                .pending
                .entry(sid.clone())
                .or_default()
                .insert(job.clone());
            let session = sid.clone();
            let root = storage::data_dir().join("attachments");
            let tx = self.attachment_imports.tx.clone();
            std::thread::spawn(move || {
                let result = attachments::import(&root, &session, source)
                    .map_err(|e| format!("添加附件失败：{e:#}"));
                let _ = tx.send(Imported {
                    session,
                    job,
                    result,
                });
            });
        }
        cx.notify();
    }

    pub(in crate::desktop::workspace) fn paste_attachments(
        &mut self,
        item: &ClipboardItem,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut sources = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(paths) => {
                    sources.extend(paths.0.iter().cloned().map(Source::File))
                }
                ClipboardEntry::Image(image) => sources.push(Source::Image {
                    bytes: image.bytes.clone(),
                    format: image.format.mime_type().into(),
                }),
                _ => {}
            }
        }
        if sources.is_empty() {
            return false;
        }
        self.import_attachments(sources, cx);
        true
    }

    pub(in crate::desktop::workspace) fn poll_attachments(&mut self, cx: &mut Context<Self>) {
        let results = self.attachment_imports.rx.try_iter().collect::<Vec<_>>();
        if results.is_empty() {
            return;
        }
        for result in results {
            if let Some(jobs) = self.attachment_imports.pending.get_mut(&result.session) {
                jobs.remove(&result.job);
            }
            let Some(session) = self.db.sessions.iter_mut().find(|s| s.id == result.session) else {
                continue;
            };
            let error = match result.result {
                Ok(attachment)
                    if session
                        .draft_attachments
                        .iter()
                        .map(|a| a.bytes)
                        .sum::<u64>()
                        + attachment.bytes
                        <= attachments::MAX_TOTAL =>
                {
                    session.draft_attachments.push(attachment);
                    None
                }
                Ok(_) => Some("附件总量不能超过 32 MiB".to_owned()),
                Err(error) => Some(error),
            };
            if let Some(error) = error {
                if self.session().is_some_and(|s| s.id == result.session) {
                    self.runtime.error = Some(error);
                } else if let Some(runtime) = self.background.get_mut(&result.session) {
                    runtime.error = Some(error);
                }
            }
        }
        self.persist();
        cx.notify();
    }
}

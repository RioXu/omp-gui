use super::*;
use gpui_kit::component::{
    attachment::{
        Attachment as AttachmentCard, AttachmentContent, AttachmentDescription, AttachmentGroup,
        AttachmentMedia, AttachmentTitle,
    },
    spinner::Spinner,
};

impl Workspace {
    pub(in crate::desktop::workspace) fn render_attachments(
        &self,
        attachments: &[Attachment],
        editable: bool,
        key: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut group = AttachmentGroup::new(SharedString::from(format!("attachments-{key}")))
            .w_full()
            .min_w_0();
        for attachment in attachments {
            let media = if attachment.mime.is_some() {
                AttachmentMedia::new().src(attachment.path.clone())
            } else {
                AttachmentMedia::new().child(Icon::new(IconName::File))
            };
            let mut card = AttachmentCard::new()
                .id(SharedString::from(format!(
                    "attachment-{key}-{}",
                    attachment.id
                )))
                .small()
                .media(media)
                .content(
                    AttachmentContent::new()
                        .title(AttachmentTitle::new(attachment.name.clone()))
                        .description(AttachmentDescription::new(format!(
                            "{:.1} KiB",
                            attachment.bytes as f64 / 1024.
                        ))),
                )
                .tooltip(
                    attachment
                        .original_path
                        .as_ref()
                        .unwrap_or(&attachment.path)
                        .to_string_lossy()
                        .into_owned(),
                );
            if editable {
                let id = attachment.id.clone();
                let owner = self.session().map(|s| s.id.clone());
                card = card.on_remove(cx.listener(move |this, _, _, cx| {
                    if let Some(s) = this.session_mut().filter(|s| Some(&s.id) == owner.as_ref()) {
                        s.draft_attachments.retain(|a| a.id != id);
                        this.persist();
                        cx.notify();
                    }
                }));
            }
            group = group.child(card);
        }
        group.into_any_element()
    }

    pub(in crate::desktop::workspace) fn render_draft_attachments(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = div().min_w_0();
        if let Some(session) = self.session()
            && !session.draft_attachments.is_empty()
        {
            content = content.child(self.render_attachments(
                &session.draft_attachments,
                !self.save_disabled,
                "draft",
                cx,
            ));
        }
        if self.importing_attachments() {
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .child(Spinner::new().xsmall())
                    .child("正在添加附件…"),
            );
        }
        content.into_any_element()
    }
}

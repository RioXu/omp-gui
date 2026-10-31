//! Open native authoring forms from settings without a raw YAML/JSON step.
use super::*;
use crate::infrastructure::configuration::authoring::{AgentDraft, RoleDraft};
use editors::{AgentEditor, EditorEvent, RoleEditor};

impl SettingsView {
    pub(super) fn open_agent(
        &mut self,
        path: Option<PathBuf>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = match path.map(|p| AgentDraft::load(&p)).transpose() {
            Ok(draft) => draft.unwrap_or_default(),
            Err(e) => {
                self.notice = format!("{e:#}");
                cx.notify();
                return;
            }
        };
        let aliases = self
            .catalog
            .entries
            .get("modelRoles")
            .map(|v| v["value"].clone())
            .unwrap_or_default();
        let view = cx.new(|cx| {
            AgentEditor::new(
                self.base(),
                draft,
                self.omp.clone(),
                self.cwd.clone(),
                aliases,
                w,
                cx,
            )
        });
        self._subscriptions
            .push(cx.subscribe(&view, |this, _, event, cx| {
                this.agent_editor = None;
                if matches!(event, EditorEvent::Saved) {
                    this.reload();
                    cx.emit(SettingsEvent::Saved);
                }
                cx.notify();
            }));
        self.agent_editor = Some(view);
        cx.notify();
    }
    pub(super) fn open_roles(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let effective = self
            .catalog
            .entries
            .get("modelRoles")
            .map(|v| v["value"].clone())
            .unwrap_or_default();
        let draft = match RoleDraft::load(settings::config_path(&self.base()), effective) {
            Ok(draft) => draft,
            Err(e) => {
                self.notice = format!("{e:#}");
                cx.notify();
                return;
            }
        };
        let view = cx.new(|cx| RoleEditor::new(draft, self.omp.clone(), self.cwd.clone(), w, cx));
        self._subscriptions
            .push(cx.subscribe(&view, |this, _, event, cx| {
                this.role_editor = None;
                if matches!(event, EditorEvent::Saved) {
                    this.reload();
                    cx.emit(SettingsEvent::Saved);
                }
                cx.notify();
            }));
        self.role_editor = Some(view);
        cx.notify();
    }
}

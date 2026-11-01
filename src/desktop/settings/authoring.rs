//! Open native authoring forms from settings without a raw YAML/JSON step.
use super::*;
use crate::infrastructure::configuration::authoring::{AgentDraft, RoleDraft};
use editors::{AgentEditor, EditorEvent, RoleEditor};

impl SettingsView {
    pub(super) fn open_agent(
        &mut self,
        path: Option<PathBuf>,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let generation = self.editor_generation;
        let category = self.category;
        let configuration = self.configuration.clone();
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = path
                .map(|path| {
                    configuration.call(ConfigAction::LoadAgent {
                        path: path.to_string_lossy().into_owned(),
                    })
                })
                .transpose()
                .map(|draft| LoadedEditor::Agent(draft.unwrap_or_default()))
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::Editor(generation, category, result));
        });
        cx.notify();
    }
    pub(super) fn show_agent(&mut self, draft: AgentDraft, w: &mut Window, cx: &mut Context<Self>) {
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
                self.configuration.clone(),
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
    pub(super) fn open_roles(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        let effective = self
            .catalog
            .entries
            .get("modelRoles")
            .map(|v| v["value"].clone())
            .unwrap_or_default();
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let generation = self.editor_generation;
        let category = self.category;
        let configuration = self.configuration.clone();
        let base = self.base().to_string_lossy().into_owned();
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = configuration
                .call(ConfigAction::LoadRoles { base, effective })
                .map(LoadedEditor::Roles)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::Editor(generation, category, result));
        });
        cx.notify();
    }
    pub(super) fn show_roles(&mut self, draft: RoleDraft, w: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| RoleEditor::new(draft, self.configuration.clone(), w, cx));
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

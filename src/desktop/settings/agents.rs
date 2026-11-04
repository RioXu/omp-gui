use super::*;
use crate::infrastructure::configuration::authoring::AgentEntry;
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};

impl SettingsView {
    pub(super) fn render_agent_manager(&self, cx: &mut Context<Self>) -> AnyElement {
        let ready = !self.catalog.root.as_os_str().is_empty() && !self.busy && !self.loading;
        let scope = if self.project {
            "project/agents"
        } else {
            "global/agents"
        };
        let entries = self
            .catalog
            .agent_files
            .get(scope)
            .cloned()
            .unwrap_or_else(|| {
                self.catalog
                    .prompt_files
                    .get(scope)
                    .into_iter()
                    .flatten()
                    .map(|path| AgentEntry {
                        path: path.clone(),
                        name: path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        ..Default::default()
                    })
                    .collect()
            });
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .child("管理当前范围的子智能体。内置定义自动导入，同名文件保留已有内容。"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "定义目录：{}",
                        self.configuration
                            .display_path(&self.configuration.join(&self.base(), "agents"))
                    )),
            )
            .child(
                div().flex().flex_wrap().gap_2().child(
                    Button::new("new-managed-agent")
                        .small()
                        .icon(IconName::Plus)
                        .label("新建子智能体")
                        .disabled(!ready)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_agent(None, window, cx)),
                        ),
                ),
            )
            .child(Input::new(&self.search).aria_label("搜索子智能体名称、职责或模型"))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("当前范围共 {} 个子智能体", entries.len())),
            );
        let mut count = 0;
        for entry in entries {
            let searchable = format!(
                "{} {} {} {}",
                entry.name,
                entry.description,
                entry.model.as_deref().unwrap_or(""),
                entry.path.display()
            )
            .to_lowercase();
            if !query.is_empty() && !searchable.contains(&query) {
                continue;
            }
            count += 1;
            let edit_path = entry.path.clone();
            let source_path = entry.path.clone();
            let remove_path = entry.path.clone();
            let name = entry.name.clone();
            let mut row = div()
                .debug_selector({
                    let name = name.clone();
                    move || format!("managed-agent-{name}")
                })
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    entry
                                        .model
                                        .clone()
                                        .unwrap_or_else(|| "继承父会话模型".into()),
                                ),
                        ),
                )
                .child(div().text_sm().child(entry.description.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.configuration.display_path(&entry.path)),
                );
            if let Some(error) = &entry.error {
                row = row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(format!("定义格式有误，可通过源码修复：{error}")),
                );
            }
            body = body.child(
                row.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new(SharedString::from(format!("edit-agent-{name}")))
                                .small()
                                .label("编辑")
                                .debug_selector({
                                    let name = name.clone();
                                    move || format!("edit-managed-agent-{name}")
                                })
                                .accessibility_label(format!("编辑子智能体 {name}"))
                                .disabled(!ready || entry.error.is_some())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_agent(Some(edit_path.clone()), window, cx)
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("source-agent-{name}")))
                                .small()
                                .ghost()
                                .label("源码")
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.file(source_path.clone(), "", window, cx)
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("remove-agent-{name}")))
                                .small()
                                .ghost()
                                .label("移除")
                                .disabled(!ready)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.request_remove_agent(remove_path.clone(), window, cx)
                                })),
                        ),
                ),
            );
        }
        if count == 0 {
            body = body.child(
                div()
                    .debug_selector(|| "managed-agents-empty".into())
                    .py_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.busy || self.loading {
                        "正在准备子智能体定义…"
                    } else if query.is_empty() {
                        "当前范围暂无子智能体"
                    } else {
                        "没有匹配的子智能体"
                    }),
            );
        }
        body.into_any_element()
    }

    pub(super) fn ensure_builtin_agents(&mut self, cx: &mut Context<Self>) {
        if self.category != "子智能体"
            || self.busy
            || self.loading
            || self.catalog.root.as_os_str().is_empty()
        {
            return;
        }
        let base = self.base();
        if !self.agent_imports.insert(base.clone()) {
            return;
        }
        let configuration = self.configuration.clone();
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = configuration
                .import_builtin_agents(&base)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::AgentsImported(base, result));
        });
        cx.notify();
    }

    fn request_remove_agent(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let configuration = self.configuration.clone();
        let base = self.base().to_string_lossy().into_owned();
        let weak = cx.entity().downgrade();
        crate::desktop::dialogs::open(window, cx, move |dialog, _, _| {
            let configuration = configuration.clone();
            let base = base.clone();
            let path = path.clone();
            let weak = weak.clone();
            dialog.title("移除自定义子智能体？")
                .child(configuration.display_path(&path))
                .child("移除当前范围的定义并保留备份。同名内置定义仍由 OMP 提供；其他范围的定义不受影响。")
                .footer(DialogFooter::new()
                    .child(DialogClose::new().trigger(|button| button.label("取消")))
                    .child(DialogAction::new().child(Button::new("confirm-remove-agent").danger().label("移除"))))
                .on_ok(move |_, _, cx| {
                    let configuration = configuration.clone();
                    let base = base.clone();
                    let path = path.to_string_lossy().into_owned();
                    let _ = weak.update(cx, |this, cx| {
                        let tx = this.tx.clone();
                        this.busy = true;
                        std::thread::spawn(move || {
                            let result = (|| -> anyhow::Result<()> {
                                let expected: String = configuration.call(ConfigAction::Read { path: path.clone() })?;
                                let _: PathBuf = configuration.call(ConfigAction::RemoveAgent { base, path, expected })?;
                                Ok(())
                            })().map_err(|error| format!("{error:#}"));
                            let _ = tx.send(ResultEvent::Saved(result));
                        });
                        cx.notify();
                    });
                    true
                })
        });
    }
}

#[cfg(test)]
mod tests;

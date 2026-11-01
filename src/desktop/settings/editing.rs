use super::*;

impl SettingsView {
    pub(super) fn server_file(&mut self, mcp: bool, cx: &mut Context<Self>) {
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let generation = self.editor_generation;
        let category = self.category;
        let configuration = self.configuration.clone();
        let base = self.base().to_string_lossy().into_owned();
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<_> {
                let names = if mcp {
                    vec!["mcp.json"]
                } else {
                    vec![
                        "lsp.json",
                        ".lsp.json",
                        "lsp.yaml",
                        ".lsp.yaml",
                        "lsp.yml",
                        ".lsp.yml",
                    ]
                }
                .into_iter()
                .map(str::to_owned)
                .collect();
                let path: PathBuf = configuration.call(ConfigAction::FindConfig { base, names })?;
                let original = configuration.read(&path)?;
                let template = if mcp {
                    "{\n  \"mcpServers\": {}\n}\n"
                } else {
                    "{\n  \"servers\": {}\n}\n"
                }
                .into();
                Ok(LoadedEditor::File {
                    path,
                    original,
                    template,
                })
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::Editor(generation, category, result));
        });
        cx.notify();
    }
    pub(super) fn base(&self) -> PathBuf {
        if self.project {
            self.configuration.join(&self.cwd, ".omp")
        } else {
            self.catalog.root.clone()
        }
    }
    pub(super) fn reload(&mut self) {
        self.loading = true;
        let tx = self.tx.clone();
        let omp = self.omp.clone();
        let configuration = self.configuration.clone();
        std::thread::spawn(move || {
            let resolved = configuration
                .call(ConfigAction::ResolveOmp { omp: omp.clone() })
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::Executable(omp.clone(), resolved));
            let _ = tx.send(ResultEvent::Catalog(
                omp,
                configuration.catalog().map_err(|e| format!("{e:#}")),
            ));
        });
    }
    pub(super) fn key(&mut self, key: String, w: &mut Window, cx: &mut Context<Self>) {
        if key == "modelRoles" {
            self.open_roles(w, cx);
            return;
        }
        let Some(meta) = self.catalog.entries.get(&key) else {
            return;
        };
        let layer = if self.project {
            &self.catalog.project
        } else {
            &self.catalog.global
        };
        let value = layer
            .get(&key)
            .and_then(|v| v.get("value"))
            .or_else(|| meta.get("value"));
        let kind = meta["type"].as_str().unwrap_or("string").to_string();
        let text = value
            .map(|v| {
                if v.is_string() {
                    v.as_str().unwrap().to_string()
                } else {
                    serde_json::to_string_pretty(v).unwrap_or_default()
                }
            })
            .unwrap_or_default();
        let text = if kind == "record" && !layer.contains_key(&key) {
            "{}".to_owned()
        } else {
            text
        };
        self.scalar
            .update(cx, |i, cx| i.set_value(text.clone(), w, cx));
        self.editor.update(cx, |i, cx| i.set_value(text, w, cx));
        self.edit = Some(Edit::Key { name: key, kind });
        cx.notify();
    }
    pub(super) fn file(
        &mut self,
        path: PathBuf,
        template: &str,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let generation = self.editor_generation;
        let category = self.category;
        let configuration = self.configuration.clone();
        let tx = self.tx.clone();
        let template = template.to_owned();
        self.busy = true;
        std::thread::spawn(move || {
            let result = configuration
                .read(&path)
                .map(|original| LoadedEditor::File {
                    path,
                    original,
                    template,
                })
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(ResultEvent::Editor(generation, category, result));
        });
        cx.notify();
    }
    pub(super) fn save(&mut self, reset: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let text = if matches!(&self.edit,Some(Edit::Key{kind,..}) if ["string","enum","number","boolean"].contains(&kind.as_str()))
        {
            self.scalar.read(cx).value().to_string()
        } else {
            self.editor.read(cx).value().to_string()
        };
        let Some(edit) = &self.edit else {
            return;
        };
        let tx = self.tx.clone();
        match edit {
            Edit::Key { name, kind } => {
                let value = if reset {
                    None
                } else {
                    match settings::parse_value(kind, &text) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            self.notice = e.to_string();
                            cx.notify();
                            return;
                        }
                    }
                };
                let project = self.project;
                let base = self.base();
                let name = name.clone();
                let configuration = self.configuration.clone();
                std::thread::spawn(move || {
                    let result = if project {
                        configuration.set_project(&base, &name, value)
                    } else {
                        match value {
                            Some(v) => configuration
                                .command(&["config", "set", &name, &v.to_string(), "--json"])
                                .map(|_| ()),
                            None => configuration
                                .command(&["config", "reset", &name, "--json"])
                                .map(|_| ()),
                        }
                    };
                    let _ = tx.send(ResultEvent::Saved(result.map_err(|e| format!("{e:#}"))));
                });
            }
            Edit::File { path, original } => {
                let path = path.clone();
                let original = original.clone();
                let configuration = self.configuration.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(ResultEvent::Saved(
                        configuration
                            .save(&path, &original, &text)
                            .map_err(|e| format!("{e:#}")),
                    ));
                });
            }
        }
        self.busy = true;
        cx.notify();
    }
    pub(super) fn plugin(&mut self, action: &'static str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name.read(cx).value().trim().to_string();
        if action != "list" && (name.is_empty() || name.starts_with('-')) {
            self.notice = "请输入明确的插件名称或路径".into();
            cx.notify();
            return;
        }
        let configuration = self.configuration.clone();
        let project = self.project;
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let mut args = vec!["plugin", action];
            if action != "list" {
                args.push(&name);
            }
            if project {
                args.push("--local");
            }
            args.push("--json");
            let result = configuration.command(&args).and_then(|s| {
                if action == "list" {
                    Ok(s)
                } else {
                    let mut args = vec!["plugin", "list", "--json"];
                    if project {
                        args.push("--local");
                    }
                    configuration.command(&args)
                }
            });
            let _ = tx.send(ResultEvent::Plugins(result.map_err(|e| format!("{e:#}"))));
        });
        cx.notify();
    }
    pub(super) fn create_skill(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_owned();
        if let Err(e) = settings::native_name(&name) {
            self.notice = e.to_string();
            cx.notify();
            return;
        }
        let path = self
            .configuration
            .join(&self.base(), &format!("skills/{name}/SKILL.md"));
        let template = format!(
            "---\nname: {name}\ndescription: 描述这个技能适用的任务\n---\n\n在这里编写技能指令。\n"
        );
        self.file(path, &template, w, cx);
    }
}

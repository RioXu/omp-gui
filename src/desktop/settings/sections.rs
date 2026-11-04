use super::*;

impl SettingsView {
    pub(super) fn render_special(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex().flex_col().gap_3();
        let ready = !self.catalog.root.as_os_str().is_empty() && !self.busy && !self.loading;
        match self.category {
            "子智能体" => return self.render_agent_manager(cx),
            "工具调用" => {
                body = body.child(div().text_sm().child("Eval 的 JavaScript 与 Python 执行分别由 eval.js、eval.py 控制；eval.tools.enabled 控制 eval 定义的工具能否用于子智能体。"))
                    .child(Button::new("find-eval-settings").small().label("定位 Eval 设置")
                        .debug_selector(|| "find-eval-settings".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search.update(cx, |input, cx| input.set_value("eval.", window, cx));
                            this.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
                            this.prepare_controls(window, cx);
                            cx.notify();
                        })));
            }
            "常规" => {
                body = body
                    .child(div().text_sm().child("OMP 可执行文件（留空自动查找）"))
                    .child(Input::new(&self.executable))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("检测结果：{}", self.resolved_executable)),
                    )
                    .child(
                        Button::new("save-executable")
                            .label("保存 OMP 路径")
                            .on_click(cx.listener(|this, _, _, cx| {
                                let value = this.executable.read(cx).value().trim().to_string();
                                let command = if value.is_empty() {
                                    "omp".into()
                                } else {
                                    value
                                };
                                let configuration = this.configuration.clone();
                                let tx = this.tx.clone();
                                this.busy = true;
                                std::thread::spawn(move || {
                                    let result = configuration
                                        .call(ConfigAction::ResolveOmp {
                                            omp: command.clone(),
                                        })
                                        .map_err(|error| format!("{error:#}"));
                                    let _ =
                                        tx.send(ResultEvent::OverrideValidated(command, result));
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("manage-ssh-connections")
                            .label("新增 SSH 连接…")
                            .on_click(
                                cx.listener(|_, _, _, cx| {
                                    cx.emit(SettingsEvent::ManageConnections)
                                }),
                            ),
                    )
                    .child(div().text_sm().child(
                        "新会话直接在项目目录执行，修改立即反映到该目录。不创建 Git 分支或工作树。",
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("当前执行目录：{}", self.cwd.display())),
                    );
            }
            "模型" if self.processing_speed => {
                body = body.child(div().text_lg().child("速度与服务等级"))
                    .child(div().text_sm().child("/fast 对应优先处理，/slow 对应 flex 或 Anthropic 低优先级。仅支持这些模式的模型生效；此处保存全局或项目的默认配置。"));
            }
            "模型" => {
                body = body
                    .child(
                        div()
                            .text_sm()
                            .child("管理提供商连接、选择模型，并配置上下文长度与输出上限。"),
                    )
                    .child(
                        Button::new("models-config")
                            .primary()
                            .label("管理模型服务…")
                            .disabled(!ready || self.project)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.open_registry(false, w, cx)),
                            ),
                    );
            }
            "网络搜索" => {
                body = body
                    .child(
                        div()
                            .text_sm()
                            .child("选择搜索服务、注册凭据并调整备用顺序。"),
                    )
                    .child(
                        Button::new("web-config")
                            .primary()
                            .label("管理搜索服务…")
                            .disabled(!ready)
                            .on_click(
                                cx.listener(|this, _, w, cx| this.open_registry(true, w, cx)),
                            ),
                    );
            }
            "LSP" | "MCP" => {
                let mcp = self.category == "MCP";
                body=body.child(div().text_sm().child(if mcp{"管理服务器地址、启动命令、环境变量及启用状态。OAuth 登录与诊断可使用 OMP 终端 /mcp。"}else{"管理语言服务器命令、文件类型、项目标记及 settings。OMP 保留内置服务器自动检测。"}))
                .child(Button::new("server-config").icon(IconName::Settings).label("管理服务器…").disabled(!ready).on_click(cx.listener(move|this,_,w,cx|{
                    let _ = w;
                    this.server_file(mcp,cx);
                })));
            }
            "角色与提示词" | "技能" => {
                let skill = self.category == "技能";
                body = body.child(div().text_sm().child(if skill {
                    "原生技能保存在 skills/<名称>/SKILL.md。下面的 skills.ignoredSkills 可停用技能，skills.customDirectories 可注册额外目录。"
                } else {
                    "配置 default、advisor 等角色与模型的关联，以及主会话系统提示词。"
                }));
                if skill {
                    body =
                        body.child(
                            div().flex().gap_2().child(Input::new(&self.name)).child(
                                Button::new("new-prompt")
                                    .icon(IconName::Plus)
                                    .label("新建技能")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.create_skill(window, cx)
                                    })),
                            ),
                        );
                }
                if !skill {
                    body = body.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("role-mapping")
                                    .small()
                                    .label("角色与模型关联…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| this.open_roles(w, cx))),
                            )
                            .child(
                                Button::new("system-prompt")
                                    .small()
                                    .label("主会话提示词…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.file(
                                            this.configuration.join(&this.base(), "SYSTEM.md"),
                                            "请在这里编写主会话的自定义系统指令。",
                                            w,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("append-prompt")
                                    .small()
                                    .label("补充系统指令…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(|this, _, w, cx| {
                                        this.file(
                                            this.configuration
                                                .join(&this.base(), "APPEND_SYSTEM.md"),
                                            "请在这里编写追加到系统提示词的指令。",
                                            w,
                                            cx,
                                        )
                                    })),
                            ),
                    );
                }
                if skill {
                    let key = format!("{}/skills", if self.project { "project" } else { "global" });
                    for path in self.catalog.prompt_files.get(&key).into_iter().flatten() {
                        let label = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let path = path.clone();
                        body = body.child(
                            Button::new(SharedString::from(format!("native-{label}")))
                                .ghost()
                                .accessibility_label(label.clone())
                                .child(div().flex_1().text_left().child(label))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.file(path.clone(), "", w, cx)
                                })),
                        );
                    }
                }
            }
            "插件" => {
                body=body.child(div().text_sm().child("通过本机 omp plugin 管理插件。安装会执行插件包安装流程；项目范围使用 --local。"))
                    .child(Input::new(&self.name));
                let mut actions = div().flex().flex_wrap().gap_2();
                for (label, action) in [
                    ("刷新", "list"),
                    ("安装", "install"),
                    ("启用", "enable"),
                    ("停用", "disable"),
                    ("卸载", "uninstall"),
                ] {
                    actions = actions.child(
                        Button::new(action)
                            .small()
                            .label(label)
                            .disabled(!ready)
                            .on_click(cx.listener(move |this, _, _, cx| this.plugin(action, cx))),
                    );
                }
                body = body.child(actions).child(
                    div()
                        .text_sm()
                        .font_family("Cascadia Code")
                        .child(self.plugins.clone()),
                );
            }
            _ => {}
        }
        body.into_any_element()
    }
}

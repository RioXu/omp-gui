use crate::{app::*, domain::*};
use chrono::{Local, TimeZone};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    diff::Diff,
    input::{Input, Textarea},
    list::ListItem,
    menu::{DropdownMenu, PopupMenuItem},
    popover::Popover,
    resizable::resize_handle,
    sidebar::{SidebarItem, SidebarMenu, SidebarMenuItem},
    text::TextView,
    tooltip::Tooltip,
    tree::Tree,
};
use gpui_kit::{prelude::FluentBuilder, *};
use serde_json::json;
#[derive(Clone)]
struct ResizeSide(bool);
impl Render for ResizeSide {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

impl Workspace {
    fn render_sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let disabled = self.running || self.creating_session || self.save_disabled;
        let mut items = div()
            .id("projects-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_4()
            .pt_2();
        for project in &self.db.projects {
            let label = project
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path = project.clone();
            let mut group = div().flex().flex_col().gap_1().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_8()
                    .child(
                        Icon::new(IconName::Folder)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    )
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(label))
                    .child(
                        Button::new(SharedString::from(format!(
                            "add-session-{}",
                            project.display()
                        )))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .accessibility_label("新建会话")
                        .tooltip("在项目内新建会话")
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.new_project_session(path.clone(), w, cx)
                        })),
                    ),
            );
            let mut sessions = self
                .db
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    &s.project == project && !s.deleted && s.archived == self.show_archived
                })
                .collect::<Vec<_>>();
            sessions.sort_by_key(|(_, s)| std::cmp::Reverse(s.updated));
            for (ix, s) in sessions {
                group = group.child(
                    div()
                        .id(SharedString::from(format!("nav-{}", s.id)))
                        .pl_3()
                        .child(
                            SidebarMenu::new()
                                .child(
                                    SidebarMenuItem::new(s.title.clone())
                                        .active(self.active == Some(ix))
                                        .disable(disabled)
                                        .context_menu({
                                            let weak = cx.entity().downgrade();
                                            let sid = s.id.clone();
                                            let archived = s.archived;
                                            move |menu, _, _| {
                                                let w1 = weak.clone();
                                                let id1 = sid.clone();
                                                let w2 = weak.clone();
                                                let id2 = sid.clone();
                                                menu.item(
                                                    PopupMenuItem::new(if archived {
                                                        "取消归档"
                                                    } else {
                                                        "归档会话"
                                                    })
                                                    .on_click(move |_, w, cx| {
                                                        let _ = w1.update(cx, |this, cx| {
                                                            this.manage_session(&id1, false, w, cx)
                                                        });
                                                    }),
                                                )
                                                .item(PopupMenuItem::new("删除会话").on_click(
                                                    move |_, w, cx| {
                                                        let _ = w2.update(cx, |this, cx| {
                                                            this.request_delete_session(
                                                                id2.clone(),
                                                                w,
                                                                cx,
                                                            )
                                                        });
                                                    },
                                                ))
                                            }
                                        })
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.activate(ix, w, cx)
                                        })),
                                )
                                .render(SharedString::from(s.id.clone()), window, cx),
                        ),
                );
            }
            if !self
                .db
                .sessions
                .iter()
                .any(|s| &s.project == project && !s.deleted && s.archived == self.show_archived)
            {
                group = group.child(
                    div()
                        .pl_9()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("点击 + 开始会话"),
                );
            }
            items = items.child(group);
        }
        div()
            .flex()
            .flex_col()
            .w(px(self.db.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .p_3()
            .gap_3()
            .child(
                div().flex().items_center().h_10().px_2().gap_2().child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("OMP"),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .h_8()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("项目"),
                    )
                    .child(
                        Button::new("add-project")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .accessibility_label("添加项目")
                            .tooltip("将目录添加为项目")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, w, cx| this.open_project(w, cx))),
                    ),
            )
            .child(items)
            .child(
                Button::new("show-archived")
                    .ghost()
                    .small()
                    .label(if self.show_archived {
                        "返回会话"
                    } else {
                        "已归档会话"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_archived = !this.show_archived;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenu::new()
                    .child(
                        SidebarMenuItem::new("设置")
                            .icon(IconName::Settings)
                            .active(self.settings_view.is_some())
                            .on_click(cx.listener(|this, _, w, cx| this.settings(w, cx))),
                    )
                    .render("sidebar-settings", window, cx),
            )
            .child(
                div()
                    .px_3()
                    .pb_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if self.demo {
                        "演示模式"
                    } else if self.connected {
                        "本地 OMP · 已连接"
                    } else {
                        "本地 OMP · 未连接"
                    }),
            )
            .into_any_element()
    }
    fn render_comment(&self, c: &Comment, cx: &mut Context<Self>) -> AnyElement {
        let cid = c.id.clone();
        let a = c.anchor.clone();
        let reply = cid.clone();
        let resolve = cid.clone();
        let theme = cx.theme().clone();
        let mut actions = div().flex().gap_2().flex_wrap().child(
            Button::new(SharedString::from(format!("anchor-{cid}")))
                .ghost()
                .xsmall()
                .label(if matches!(a.source, Source::File { .. }) {
                    "查看当时变更"
                } else {
                    "定位原文"
                })
                .on_click(cx.listener(move |this, _, _, cx| this.open_anchor(a.clone(), cx))),
        );
        if let Source::File { path, .. } = &c.anchor.source {
            let path = path.clone();
            actions = actions.child(
                Button::new(SharedString::from(format!("current-{cid}")))
                    .ghost()
                    .xsmall()
                    .label("打开当前代码")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.viewed_path = Some(path.clone());
                        this.panel_open = true;
                        this.panel_tab = "变更";
                        this.open_current();
                        cx.notify();
                    })),
            );
        }
        let info = if c.resolved {
            "你已标记解决"
        } else if c.reply_ids.is_empty() {
            "待回应"
        } else {
            "已有跟进 · 待确认"
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_2()
            .mb_3()
            .p_3()
            .bg(theme.muted)
            .rounded_lg()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} · {}", c.anchor.label(), &cid[..8])),
            )
            .child(TextView::markdown(
                SharedString::from(format!("quote-{cid}")),
                format!("> {}", c.anchor.quote.replace('\n', "\n> ")),
            ))
            .child(c.text.clone())
            .child(actions)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(info)
                    .child(
                        Button::new(SharedString::from(format!("follow-{cid}")))
                            .ghost()
                            .xsmall()
                            .label("继续跟进")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.follow_up = Some(reply.clone());
                                this.input.focus_handle(cx).focus(w, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("resolve-{cid}")))
                            .ghost()
                            .xsmall()
                            .label(if c.resolved {
                                "重新打开"
                            } else {
                                "标记已解决"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.session_mut()
                                    && let Some(c) = s.comments.iter_mut().find(|c| c.id == resolve)
                                {
                                    c.resolved = !c.resolved;
                                }
                                this.persist();
                                this.invalidate_list();
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
    pub fn render_message(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.session() else {
            return div().into_any_element();
        };
        let Some(m) = s.messages.get(ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let mut content = div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .py_3();
        let label = if m.role == "user" { "你" } else { "⌥ OMP" };
        content = content.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        );
        if !m.text.trim().is_empty()
            && let Some(view) = self.texts.get(&m.id)
        {
            let row = div()
                .w_full()
                .when(m.role == "user", |d| d.bg(theme.muted).rounded_lg().p_3())
                .child(TextView::new(&view.state));
            content = content.child(row);
        }
        for cid in &m.comments {
            if let Some(c) = s.comments.iter().find(|c| &c.id == cid) {
                content = content.child(self.render_comment(c, cx));
            }
        }
        if m.role == "assistant" {
            for c in s.comments.iter().filter(|c| c.reply_ids.contains(&m.id)) {
                let a = c.anchor.clone();
                content = content.child(
                    Button::new(SharedString::from(format!("reply-{}-{}", m.id, c.id)))
                        .ghost()
                        .xsmall()
                        .label(format!("↗ 回应评论 {}", &c.id[..8]))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open_anchor(a.clone(), cx)),
                        ),
                );
            }
        }
        div()
            .id(SharedString::from(format!("message-time-{}", m.id)))
            .w_full()
            .px_6()
            .tooltip({
                let time = Local
                    .timestamp_opt(m.time as i64, 0)
                    .single()
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();
                move |w, cx| Tooltip::new(time.clone()).build(w, cx)
            })
            .child(content)
            .into_any_element()
    }
    fn render_pending(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.session() else {
            return div().into_any_element();
        };
        let pending: Vec<_> = s.comments.iter().filter(|c| !c.sent).collect();
        if pending.is_empty() {
            return div().into_any_element();
        }
        let mut box_ = div().flex().flex_col().gap_2().child(
            Button::new("pending-toggle")
                .ghost()
                .small()
                .label(format!(
                    "待发送评论 · {}  {}",
                    pending.len(),
                    if self.pending_open { "⌃" } else { "⌄" }
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pending_open = !this.pending_open;
                    cx.notify();
                })),
        );
        if self.pending_open {
            let mut list = div()
                .id("pending-comments")
                .max_h(rems(12.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            for c in pending {
                let cid = c.id.clone();
                let edit = c.clone();
                list = list.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(c.anchor.label()),
                        )
                        .child(c.text.clone())
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new(SharedString::from(format!("edit-{cid}")))
                                        .ghost()
                                        .xsmall()
                                        .label("编辑")
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.comment_editor = Some(edit.anchor.clone());
                                            this.editing_comment = Some(edit.id.clone());
                                            this.selection_point = w.mouse_position();
                                            this.comment_input.update(cx, |i, cx| {
                                                i.set_value(edit.text.clone(), w, cx)
                                            });
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("delete-{cid}")))
                                        .ghost()
                                        .xsmall()
                                        .label("删除")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(s) = this.session_mut() {
                                                s.comments.retain(|c| c.id != cid || c.sent);
                                            }
                                            this.persist();
                                            cx.notify();
                                        })),
                                ),
                        ),
                );
            }
            box_ = box_.child(list);
        }
        box_.into_any_element()
    }
    fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let models = self.models.clone();
        let weak = cx.entity().downgrade();
        let model=Button::new("model-picker").ghost().small().label(self.model_label.clone()).disabled(self.running||models.is_empty()).dropdown_menu(move|mut menu,_,_|{for m in &models{let provider=m["provider"].as_str().unwrap_or("").to_string();let model=m["id"].as_str().unwrap_or("").to_string();let label=format!("{provider}/{model}");let weak=weak.clone();menu=menu.item(PopupMenuItem::new(label).on_click(move|_,_,cx|{let _=weak.update(cx,|this,cx|{this.command(json!({"type":"set_model","id":id(),"provider":provider,"modelId":model}));cx.notify();});}));}menu});
        let weak = cx.entity().downgrade();
        let thinking = Button::new("thinking")
            .ghost()
            .small()
            .label(format!("推理：{}", self.thinking))
            .disabled(self.running || !self.connected)
            .dropdown_menu(move |mut menu, _, _| {
                for level in ["off", "low", "medium", "high", "xhigh"] {
                    let weak = weak.clone();
                    menu = menu.item(PopupMenuItem::new(level).on_click(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.command(
                                json!({"type":"set_thinking_level","id":id(),"level":level}),
                            );
                            cx.notify();
                        });
                    }));
                }
                menu
            });
        let mut area = div().flex().flex_col().gap_2().px_6().pb_4().pt_2();
        let mut inner = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(rems(52.))
            .mx_auto()
            .gap_2()
            .child(self.render_pending(cx));
        if let Some(cid) = &self.follow_up {
            inner = inner.child(
                div()
                    .flex()
                    .gap_2()
                    .text_xs()
                    .child(format!("跟进评论 {}", &cid[..8]))
                    .child(
                        Button::new("cancel-follow")
                            .ghost()
                            .xsmall()
                            .label("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.follow_up = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        inner = inner
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_xl()
                    .bg(theme.muted)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        Textarea::new(&self.input)
                            .appearance(false)
                            .aria_label("给 OMP 的指令"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(model)
                            .child(thinking)
                            .child(div().flex_1())
                            .child(if self.running {
                                Button::new("stop")
                                    .small()
                                    .label("停止")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.command(json!({"type":"abort","id":id()}));
                                        this.status = "正在停止…".into();
                                        cx.notify();
                                    }))
                            } else {
                                Button::new("send")
                                    .primary()
                                    .small()
                                    .icon(IconName::ArrowUp)
                                    .label("发送")
                                    .tooltip("Ctrl+Enter")
                                    .on_click(cx.listener(|this, _, w, cx| this.send(w, cx)))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if self.demo {
                        "演示模式，不调用模型".into()
                    } else {
                        self.status.clone()
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("todo-shortcut")
                            .ghost()
                            .xsmall()
                            .label("任务")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel_open = true;
                                this.panel_tab = "任务";
                                cx.notify();
                            })),
                    )
                    .child(format!("上下文 {:.0}%", self.context_percent)),
            );
        area = area.child(inner);
        area.into_any_element()
    }
    fn render_requests(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(r) = self.requests.front() else {
            return div().into_any_element();
        };
        let method = r["method"].as_str().unwrap_or("");
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .mx_6()
            .bg(cx.theme().muted)
            .rounded_lg()
            .child(
                r["title"]
                    .as_str()
                    .unwrap_or("OMP 需要你的回应")
                    .to_string(),
            );
        if let Some(message) = r["message"].as_str() {
            body = body.child(message.to_string());
        }
        if method == "select" {
            for (ix, v) in r["options"].as_array().into_iter().flatten().enumerate() {
                let text = v.as_str().unwrap_or("").to_string();
                let value = text.clone();
                body = body.child(
                    Button::new(SharedString::from(format!("request-option-{ix}")))
                        .small()
                        .label(text)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.answer_request(Some(value.clone()), None);
                            cx.notify();
                        })),
                );
            }
        } else if method == "confirm" {
            body = body.child(
                Button::new("approve")
                    .small()
                    .label("允许")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.answer_request(None, Some(true));
                        cx.notify();
                    })),
            );
        } else {
            body = body.child(Input::new(&self.answer_input)).child(
                Button::new("answer")
                    .small()
                    .label("提交回答")
                    .on_click(cx.listener(|this, _, w, cx| {
                        let v = this.answer_input.read(cx).value().to_string();
                        this.answer_request(Some(v), None);
                        this.answer_input.update(cx, |i, cx| i.set_value("", w, cx));
                        cx.notify();
                    })),
            );
        }
        body.child(
            Button::new("deny")
                .ghost()
                .small()
                .label("取消 / 拒绝")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.answer_request(None, None);
                    cx.notify();
                })),
        )
        .into_any_element()
    }
    fn render_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(self.db.panel_width))
            .min_w(rems(18.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.background)
            .border_l_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_3()
                    .h_12()
                    .child("工作面板")
                    .child(div().flex_1())
                    .child(
                        Button::new("close-panel")
                            .ghost()
                            .small()
                            .label("收起")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel_open = false;
                                this.selected = None;
                                cx.notify();
                            })),
                    ),
            );
        let mut tabs = div().flex().px_2().gap_1();
        for label in ["变更", "文件", "任务", "子智能体"] {
            tabs = tabs.child(
                Button::new(label)
                    .ghost()
                    .small()
                    .selected(self.panel_tab == label)
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.panel_tab = label;
                        this.selected = None;
                        if label == "文件" || label == "变更" {
                            this.refresh_files(cx);
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(tabs);
        let mut body = div()
            .id("work-panel-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2();
        match self.panel_tab {
            "任务" => {
                if self.todos.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("OMP 尚未创建任务清单"),
                    );
                }
                for phase in &self.todos {
                    body = body.child(phase["name"].as_str().unwrap_or("任务").to_string());
                    for task in phase["tasks"].as_array().into_iter().flatten() {
                        let status = match task["status"].as_str().unwrap_or("") {
                            "completed" => "✓",
                            "in_progress" => "进行中",
                            "blocked" => "受阻",
                            "abandoned" => "取消",
                            _ => "待处理",
                        };
                        body = body.child(div().text_sm().py_2().child(format!(
                            "{status}  {}",
                            task["content"].as_str().unwrap_or("")
                        )));
                    }
                }
            }
            "子智能体" => {
                if self.agents.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("本次会话没有子智能体"),
                    );
                }
                for agent in &self.agents {
                    let name = agent["name"]
                        .as_str()
                        .or_else(|| agent["agent"].as_str())
                        .unwrap_or("子智能体");
                    body = body.child(div().py_2().child(format!(
                        "{name} · {}",
                        agent["status"].as_str().unwrap_or("运行中")
                    )));
                }
            }
            _ => {
                let paths = if self.panel_tab == "文件" {
                    &self.files
                } else {
                    &self.changed
                };
                body = body.child(
                    Button::new("refresh-files")
                        .ghost()
                        .xsmall()
                        .label(if self.loading_files {
                            "正在刷新…"
                        } else {
                            "刷新文件"
                        })
                        .disabled(self.loading_files)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.refresh_files(cx);
                            cx.notify();
                        })),
                );
                let tree_state = if self.panel_tab == "文件" {
                    &self.file_tree
                } else {
                    &self.change_tree
                };
                let weak = cx.entity().downgrade();
                body = body.child(
                    div()
                        .h(if self.panel_text.is_some() {
                            rems(12.)
                        } else {
                            rems(32.)
                        })
                        .min_h(rems(6.))
                        .flex_shrink_0()
                        .child(
                            Tree::new(tree_state, move |_, entry, _, _, cx| {
                                let folder = entry.is_folder();
                                let path = entry.item().id.to_string();
                                let weak = weak.clone();
                                ListItem::new(entry.item().id.clone())
                                    .h_7()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .pl(px(entry.depth() as f32 * 14.))
                                            .child(
                                                Icon::new(if folder {
                                                    if entry.is_expanded() {
                                                        IconName::ChevronDown
                                                    } else {
                                                        IconName::ChevronRight
                                                    }
                                                } else {
                                                    IconName::File
                                                })
                                                .size_4()
                                                .text_color(cx.theme().muted_foreground),
                                            )
                                            .when(folder, |d| {
                                                d.child(Icon::new(IconName::Folder).size_4())
                                            })
                                            .child(entry.item().label.clone()),
                                    )
                                    .when(!folder, |row| {
                                        row.on_click(move |_, _, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                this.open_file(path.clone().into());
                                                cx.notify();
                                            });
                                        })
                                    })
                            })
                            .size_full(),
                        ),
                );
                if let Some(entry) = &self.panel_text {
                    if let Some(quote) = &self.viewed_quote {
                        body = body.child(
                            div()
                                .id("historical-quote")
                                .max_h_24()
                                .overflow_y_scroll()
                                .p_3()
                                .rounded_md()
                                .bg(theme.muted)
                                .border_l_2()
                                .border_color(theme.primary)
                                .text_sm()
                                .child("评论时选中的片段")
                                .child(
                                    div()
                                        .mt_2()
                                        .font_family("Cascadia Code")
                                        .child(quote.clone()),
                                ),
                        );
                    }
                    body = body.child(
                        div()
                            .pt_3()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if self.historical {
                                "评论时版本 · 原始引用保持不变"
                            } else {
                                "当前读取快照 · 后续修改不会覆盖它"
                            }),
                    );
                    if self.viewed_path.is_some() {
                        body = body.child(
                            div().flex().gap_2().child(
                                Button::new("current-code")
                                    .ghost()
                                    .xsmall()
                                    .label("打开当前代码")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_current();
                                        cx.notify();
                                    })),
                            ),
                        );
                    }
                    body = if let Some(diff) = &self.diff {
                        body.child(
                            div()
                                .h(rems(30.))
                                .min_h(rems(18.))
                                .flex_shrink_0()
                                .border_1()
                                .border_color(theme.border)
                                .rounded_md()
                                .overflow_hidden()
                                .child(Diff::new(diff).size_full()),
                        )
                    } else {
                        body.child(TextView::new(&entry.state))
                    };
                } else if paths.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("暂无文件变更；也可以切换到文件页查看"),
                    );
                }
            }
        }
        panel = panel.child(body);
        panel.into_any_element()
    }
    pub fn render_comment_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(anchor) = &self.comment_editor else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .w_80()
            .bg(cx.theme().popover.opacity(1.0))
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .shadow_lg()
            .child(div().text_sm().child(anchor.label()))
            .child(
                div()
                    .max_h_32()
                    .id("comment-quote")
                    .overflow_y_scroll()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(anchor.quote.clone()),
            )
            .child(Textarea::new(&self.comment_input).aria_label("评论内容"))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel-comment")
                            .ghost()
                            .small()
                            .label("取消")
                            .on_click(cx.listener(|this, _, w, cx| {
                                this.comment_editor = None;
                                this.selected = None;
                                this.input.focus_handle(cx).focus(w, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("save-comment")
                            .primary()
                            .small()
                            .label("加入待发送")
                            .on_click(cx.listener(|this, _, w, cx| this.save_comment(w, cx))),
                    ),
            )
            .into_any_element()
    }
    pub fn invalidate_list(&self) {
        self.list
            .reset(self.session().map(|s| s.messages.len()).unwrap_or(0));
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let width = window.viewport_size().width;
        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .flex()
            .size_full()
            .items_stretch()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_base()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selection_dirty = true),
            )
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<ResizeSide>, w, cx| {
                let left = e.drag(cx).0;
                let x: f32 = e.event.position.x.into();
                let width: f32 = w.viewport_size().width.into();
                if left {
                    let panel = if this.panel_open {
                        this.db.panel_width
                    } else {
                        0.
                    };
                    this.db.sidebar_width = x.clamp(170., (width - panel - 360.).clamp(170., 420.));
                } else {
                    let sidebar = if width > 1000. {
                        this.db.sidebar_width
                    } else {
                        0.
                    };
                    this.db.panel_width =
                        (width - x).clamp(280., (width - sidebar - 360.).max(280.));
                }
                this.dirty = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SendPrompt, w, cx| this.send(w, cx)))
            .on_action(cx.listener(|this, _: &AddComment, w, cx| this.begin_comment(w, cx)))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| {
                this.panel_open = !this.panel_open;
                if this.panel_open {
                    this.refresh_files(cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewSession, w, cx| this.new_session(w, cx)))
            .on_action(cx.listener(|this, _: &OpenProject, w, cx| this.open_project(w, cx)));
        if width > px(1000.) || !self.panel_open {
            root = root.child(self.render_sidebar(window, cx)).child(
                div().relative().w_0().h_full().flex_shrink_0().child(
                    resize_handle("sidebar-resize", Axis::Horizontal)
                        .on_drag(ResizeSide(true), |d, _, _, cx| cx.new(|_| d.clone())),
                ),
            );
        }
        if let Some(settings) = &self.settings_view {
            return root.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h_12()
                            .px_4()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(div().flex_1().child("设置"))
                            .child(
                                Button::new("close-settings")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Close)
                                    .tooltip("返回会话")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings_view = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(settings.clone())),
            );
        }
        let title = self
            .session()
            .map(|s| s.title.clone())
            .unwrap_or("OMP".into());
        let cwd = self
            .session()
            .map(|s| s.cwd().display().to_string())
            .unwrap_or_default();
        let branch = self
            .session()
            .and_then(|s| s.branch.clone())
            .unwrap_or_default();
        let location = Button::new("session-location")
            .ghost()
            .xsmall()
            .label(if branch.is_empty() {
                "原目录"
            } else {
                "工作树"
            })
            .tooltip(cwd.clone())
            .dropdown_menu(move |menu, _, _| {
                let path = cwd.clone();
                let branch = branch.clone();
                menu.item(
                    PopupMenuItem::new("复制执行目录").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                    }),
                )
                .when(!branch.is_empty(), |m| {
                    m.item(
                        PopupMenuItem::new("复制工作树分支").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(branch.clone()))
                        }),
                    )
                })
            });
        let mut center = div().flex().flex_col().h_full().flex_1().min_w_0().child(
            div()
                .flex()
                .items_center()
                .h_12()
                .px_4()
                .gap_2()
                .child(div().flex_1().min_w_0().truncate().child(title))
                .child(location)
                .child(
                    Button::new("panel-toggle")
                        .ghost()
                        .small()
                        .icon(IconName::PanelRight)
                        .label("工作面板")
                        .selected(self.panel_open)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.panel_open = !this.panel_open;
                            if this.panel_open {
                                this.refresh_files(cx);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("theme")
                        .ghost()
                        .small()
                        .icon(IconName::Sun)
                        .tooltip("切换深浅色")
                        .on_click(cx.listener(|this, _, w, cx| this.toggle_theme(w, cx))),
                ),
        );
        if let Some(error) = &self.error {
            center = center.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mx_4()
                    .p_2()
                    .rounded_md()
                    .bg(theme.danger.opacity(0.1))
                    .child(div().flex_1().child(error.clone()))
                    .child(
                        Button::new("dismiss-error")
                            .ghost()
                            .xsmall()
                            .label("关闭")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.session().is_some_and(|s| !s.messages.is_empty()) {
            let weak = cx.entity().downgrade();
            center = center.child(
                list(self.list.clone(), move |ix, _, cx| {
                    weak.update(cx, |this, cx| this.render_message(ix, cx))
                        .unwrap_or_else(|_| div().into_any_element())
                })
                .flex_1()
                .min_h_0(),
            );
        } else {
            center = center.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(div().text_2xl().child(if self.session().is_some() {
                        "描述你想完成的工作。"
                    } else {
                        "从这里开始。"
                    }))
                    .child(div().text_color(theme.muted_foreground).child(
                        if self.session().is_some() {
                            "任务将在本会话的项目目录中执行。"
                        } else {
                            "添加项目，再点击项目右侧 + 创建会话。"
                        },
                    ))
                    .child(
                        Button::new("empty-open")
                            .label("打开项目…")
                            .on_click(cx.listener(|this, _, w, cx| this.open_project(w, cx))),
                    ),
            );
        }
        center = center
            .child(self.render_requests(cx))
            .child(self.render_composer(cx));
        root = root.child(center);
        if self.panel_open {
            root = root
                .child(
                    div().relative().w_0().h_full().flex_shrink_0().child(
                        resize_handle("work-panel-resize", Axis::Horizontal)
                            .on_drag(ResizeSide(false), |d, _, _, cx| cx.new(|_| d.clone())),
                    ),
                )
                .child(self.render_panel(cx));
        }
        if self.selected.is_some() || self.comment_editor.is_some() {
            let weak = cx.entity().downgrade();
            let content_weak = weak.clone();
            let open = self.comment_editor.is_some();
            let x = self
                .selection_point
                .x
                .min((width - px(370.)).max(px(8.)))
                .max(px(8.));
            let y = (self.selection_point.y - px(36.)).max(px(50.));
            root = root.child(
                div().absolute().left(x).top(y).child(
                    Popover::new("selection-comment-popover")
                        .appearance(false)
                        .anchor(gpui_kit::Anchor::TopLeft)
                        .open(open)
                        .trigger(
                            Button::new("selection-comment")
                                .small()
                                .bg(theme.popover.opacity(1.0))
                                .icon(IconName::Plus)
                                .label("评论")
                                .tooltip("Ctrl+Alt+M"),
                        )
                        .on_open_change(move |open, w, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if *open {
                                    this.begin_comment(w, cx);
                                } else {
                                    this.comment_editor = None;
                                    this.selected = None;
                                    cx.notify();
                                }
                            });
                        })
                        .content(move |_, _, cx| {
                            content_weak
                                .update(cx, |this, cx| this.render_comment_editor(cx))
                                .unwrap_or_else(|_| div().into_any_element())
                        }),
                ),
            );
        }
        root
    }
}

use super::*;

impl Workspace {
    pub fn on_frame(&mut self, f: Value, window: &mut Window, cx: &mut Context<Self>) {
        let kind = f["type"].as_str().unwrap_or("");
        match kind {
            "ready" => {
                self.command(
                    json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2}),
                );
                self.command(
                    json!({"type":"set_subagent_subscription","id":"subagents","level":"progress"}),
                );
                self.command(json!({"type":"get_available_models","id":"models"}));
                self.request_state();
            }
            "response" => {
                if f["success"] == false {
                    let command = f["command"].as_str().unwrap_or("");
                    if ![
                        "negotiate_protocol",
                        "set_subagent_subscription",
                        "get_subagents",
                    ]
                    .contains(&command)
                    {
                        self.error = Some(f["error"].as_str().unwrap_or("OMP 请求失败").into());
                    }
                    if command == "prompt" {
                        self.running = false;
                        self.pending_send = None;
                    }
                    return;
                }
                match f["command"].as_str().unwrap_or("") {
                    "get_state" => {
                        let d = &f["data"];
                        self.connected = true;
                        self.starting = false;
                        self.status = if self.running {
                            self.activity
                                .back()
                                .cloned()
                                .unwrap_or("OMP 正在思考…".into())
                        } else {
                            "本地 OMP 已连接".into()
                        };
                        self.model_label = d["model"]["id"].as_str().unwrap_or("未配置模型").into();
                        self.thinking = d["thinkingLevel"].as_str().unwrap_or("自动").into();
                        self.context_percent = d["contextUsage"]["percent"].as_f64().unwrap_or(0.);
                        self.todos = d["todoPhases"].as_array().cloned().unwrap_or_default();
                        if let Some(title) =
                            d["sessionName"].as_str().filter(|s| !s.trim().is_empty())
                            && let Some(s) = self.session_mut()
                            && s.title != title
                        {
                            s.title = title.into();
                            self.dirty = true;
                        }
                        if let Some(file) = d["sessionFile"].as_str()
                            && let Some(s) = self.session_mut()
                            && s.omp_session.as_deref() != Some(file)
                        {
                            s.omp_session = Some(file.into());
                            self.dirty = true;
                        }
                    }
                    "get_available_models" => {
                        self.models = f["data"]["models"]
                            .as_array()
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    "get_subagents" => {
                        self.agents = f["data"]["subagents"]
                            .as_array()
                            .or_else(|| f["data"]["agents"].as_array())
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    "prompt" => {
                        self.text_update_pending = true;
                        if let Some((id, text)) = self.pending_send.take() {
                            if f["id"] == id {
                                let follow = self.follow_up.take();
                                if let Some(s) = self.session_mut() {
                                    if s.title == "新建会话" {
                                        s.title = text.chars().take(28).collect();
                                        if s.title.trim().is_empty() {
                                            s.title = "评论跟进".into();
                                        }
                                    }
                                    let (_, ids) = s.submit(text, follow);
                                    self.in_flight_comments = ids;
                                }
                                self.input.update(cx, |i, cx| i.set_value("", window, cx));
                                if let Some(s) = self.session_mut() {
                                    s.draft.clear();
                                }
                                self.persist();
                            } else {
                                self.pending_send = Some((id, text));
                            }
                        }
                        if f["data"]["agentInvoked"] == false {
                            self.running = false;
                        }
                    }
                    "set_model" | "set_thinking_level" => self.request_state(),
                    _ => {}
                }
            }
            "agent_start" => {
                self.running = true;
                self.status = "OMP 正在处理…".into();
            }
            "session_info_update" => {
                if let Some(title) = f["title"].as_str().filter(|s| !s.trim().is_empty())
                    && let Some(s) = self.session_mut()
                {
                    s.title = title.into();
                    self.dirty = true;
                }
            }
            "agent_end" => {
                self.request_state();
                self.refresh_files(cx);
            }
            "session_settled" | "prompt_result" => {
                self.running = false;
                if let Some(s) = self.session_mut() {
                    for block in s.messages.iter_mut().flat_map(|m| &mut m.process) {
                        block.streaming = false;
                    }
                }
                self.invalidate_list();
                self.status = "本轮已完成".into();
                if f["status"] == "error" || f["status"] == "aborted" {
                    self.status = "本轮已停止".into();
                    if let Some(e) = f["error"].as_str() {
                        self.error = Some(e.into());
                    }
                }
                self.request_state();
                self.refresh_files(cx);
                self.persist();
            }
            "message_start" | "message_update" | "message_end" => {
                self.text_update_pending = true;
                let m = &f["message"];
                if m["role"] != "assistant" {
                    return;
                }
                let text = crate::engine::message_text(m);
                let mut process = process_blocks(
                    m,
                    kind,
                    f["assistantMessageEvent"]["type"].as_str().unwrap_or(""),
                );
                if text.is_empty() && process.is_empty() {
                    return;
                }
                let rid = f["messageId"].as_str().unwrap_or("stream").to_string();
                let mid = self.rpc_messages.entry(rid).or_insert_with(id).clone();
                let linked = self.in_flight_comments.clone();
                if let Some(s) = self.session_mut() {
                    if let Some(existing) = s.messages.iter_mut().find(|m| m.id == mid) {
                        existing.text = text;
                        for block in &mut process {
                            block.result = existing
                                .process
                                .iter()
                                .find(|b| b.id == block.id)
                                .and_then(|b| b.result.clone());
                        }
                        existing.process = process;
                    } else {
                        let mut msg = Message::new("assistant", text);
                        msg.id = mid.clone();
                        msg.process = process;
                        s.messages.push(msg);
                        for c in &mut s.comments {
                            if linked.contains(&c.id) && !c.reply_ids.contains(&mid) {
                                c.reply_ids.push(mid.clone());
                            }
                        }
                    }
                }
                self.dirty = true;
                if let Some(ix) = self
                    .session()
                    .and_then(|s| s.messages.iter().position(|m| m.id == mid))
                {
                    let count = self.list.item_count();
                    if ix < count {
                        self.list.splice(ix..ix + 1, 1);
                    } else if count > 0 {
                        self.list.splice(count - 1..count, 1);
                    }
                }
            }
            "tool_execution_start" => {
                let name = f["toolName"].as_str().unwrap_or("工具");
                self.activity.push_back(format!("正在执行 {name}"));
                if self.activity.len() > 200 {
                    self.activity.pop_front();
                }
            }
            "tool_execution_end" => {
                let call_id = f["toolCallId"].as_str().unwrap_or_default();
                if let Some(s) = self.session_mut() {
                    for block in s
                        .messages
                        .iter_mut()
                        .flat_map(|m| &mut m.process)
                        .filter(|b| b.kind == ProcessKind::Tool && b.id == call_id)
                    {
                        block.result = Some(crate::engine::message_text(&f["result"]));
                    }
                }
                self.dirty = true;
                self.invalidate_list();
                self.request_state();
            }
            "subagent_lifecycle" | "subagent_progress" => {
                self.command(json!({"type":"get_subagents","id":"agents"}));
            }
            "extension_ui_request" => {
                match f["method"].as_str().unwrap_or("") {
                    "select" | "confirm" | "input" | "editor" => {
                        if f["secret"] == true {
                            self.command(json!({"type":"extension_ui_response","id":f["id"],"cancelled":true}));
                            self.error = Some("请在 OMP 终端完成敏感凭据输入后重新连接".into());
                        } else {
                            self.requests.push_back(f);
                        }
                    }
                    "cancel" => {
                        self.requests.retain(|r| r["id"] != f["targetId"]);
                    }
                    "notify" => {
                        self.status = f["message"].as_str().unwrap_or("").into();
                    }
                    "set_editor_text" => {
                        if let Some(text) = f["text"].as_str() {
                            self.input
                                .update(cx, |i, cx| i.set_value(text.to_string(), window, cx));
                        }
                    }
                    _ => {}
                }
            }
            "notice" | "extension_error" | "rpc_frame_error" => {
                self.error = Some(
                    f["message"]
                        .as_str()
                        .or_else(|| f["error"].as_str())
                        .unwrap_or("OMP 返回一条异常通知")
                        .into(),
                );
            }
            _ => {}
        }
    }
}

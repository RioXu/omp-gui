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
                        self.accept_submission(&f, window, cx);
                    }
                    if command == "abort_and_restore_queue" {
                        self.stopping = false;
                    }
                    if command == "promote_queued_message" {
                        self.steering_request = None;
                        self.request_state();
                    }
                    return;
                }
                match f["command"].as_str().unwrap_or("") {
                    "promote_queued_message" => {
                        self.steering_request = None;
                        self.status = if f["data"]["promoted"] == true {
                            "指示已提交，接收时机由 OMP 决定"
                        } else {
                            "该指令已被模型接收"
                        }
                        .into();
                        self.request_state();
                    }
                    "get_state" => {
                        let d = &f["data"];
                        self.connected = true;
                        self.starting = false;
                        if d["isSettled"].is_boolean() && self.pending_send.is_none() {
                            self.running = d["isSettled"] == false;
                        }
                        self.queued_inputs = ["followUp"]
                            .into_iter()
                            .flat_map(|key| {
                                d["queuedMessages"][key].as_array().into_iter().flatten()
                            })
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect();
                        self.steered_inputs = d["queuedMessages"]["steering"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect();
                        self.status = if self.running {
                            self.active_tools
                                .values()
                                .next()
                                .map(|name| format!("正在执行 {name}"))
                                .unwrap_or("OMP 正在思考…".into())
                        } else if self.interrupted_run {
                            "已停止".into()
                        } else {
                            "本地 OMP 已连接".into()
                        };
                        self.model_label = d["model"]["id"].as_str().unwrap_or("未配置模型").into();
                        self.thinking = d["thinkingLevel"].as_str().unwrap_or("自动").into();
                        self.context_percent = d["contextUsage"]["percent"].as_f64().unwrap_or(0.);
                        self.context_limit = d["contextUsage"]["contextWindow"]
                            .as_u64()
                            .or_else(|| d["model"]["contextWindow"].as_u64())
                            .unwrap_or(0);
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
                    "prompt" => self.accept_submission(&f, window, cx),
                    "abort_and_restore_queue" => self.restore_queue(&f, window, cx),
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
            "queue_update" => self.request_state(),
            "session_settled" | "prompt_result" => {
                if kind == "prompt_result" && f["sessionSettled"] == false {
                    self.request_state();
                    return;
                }
                self.queued_inputs.clear();
                self.steered_inputs.clear();
                self.running = false;
                self.active_tools.clear();
                self.interrupted_run |= f["status"] == "aborted";
                if self.interrupted_run {
                    let start = self.run_message_start;
                    if let Some(s) = self.session_mut()
                        && let Some(m) = s
                            .messages
                            .iter_mut()
                            .skip(start)
                            .rev()
                            .find(|m| m.role == "assistant")
                    {
                        m.state = MessageState::Interrupted;
                    }
                }
                if let Some(s) = self.session_mut() {
                    for block in s.messages.iter_mut().flat_map(|m| &mut m.process) {
                        block.streaming = false;
                    }
                }
                self.refresh_transcript();
                self.status = if self.interrupted_run {
                    "已停止"
                } else {
                    "本轮已完成"
                }
                .into();
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
                if m["role"] == "user" {
                    self.queue_delivered(&crate::engine::message_text(m));
                }
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
                            block.patch = existing
                                .process
                                .iter()
                                .find(|b| b.id == block.id)
                                .and_then(|b| b.patch.clone());
                            block.result = existing
                                .process
                                .iter()
                                .find(|b| b.id == block.id)
                                .and_then(|b| b.result.clone());
                        }
                        for previous in &existing.process {
                            if previous.kind == ProcessKind::Tool
                                && !previous.id.is_empty()
                                && !process.iter().any(|b| b.id == previous.id)
                            {
                                process.push(previous.clone());
                            }
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
            }
            "tool_execution_start" => {
                let name = f["toolName"].as_str().unwrap_or("工具");
                let call_id = f["toolCallId"].as_str().unwrap_or_default().to_owned();
                self.active_tools.insert(call_id.clone(), name.into());
                let start = self.run_message_start;
                if let Some(s) = self.session_mut() {
                    s.start_tool(&call_id, name, &f["args"], start);
                }
                self.text_update_pending = true;
            }
            "tool_execution_end" => {
                let call_id = f["toolCallId"].as_str().unwrap_or_default();
                self.active_tools.remove(call_id);
                if let Some(s) = self.session_mut() {
                    for block in s
                        .messages
                        .iter_mut()
                        .flat_map(|m| &mut m.process)
                        .filter(|b| b.kind == ProcessKind::Tool && b.id == call_id)
                    {
                        block.set_result(
                            crate::engine::message_text(&f["result"]),
                            &f["result"]["details"],
                        );
                    }
                }
                self.dirty = true;
                self.refresh_transcript();
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

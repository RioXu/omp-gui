use super::*;

impl Workspace {
    pub fn on_frame(&mut self, f: Value, window: &mut Window, cx: &mut Context<Self>) {
        let kind = f["type"].as_str().unwrap_or("");
        if matches!(
            kind,
            "message_start"
                | "message_update"
                | "message_end"
                | "agent_start"
                | "agent_end"
                | "session_settled"
                | "prompt_result"
        ) {
            self.runtime.last_activity = Instant::now();
        }
        match kind {
            "ready" => {
                self.command(
                    json!({"type":"negotiate_protocol","id":"protocol","protocolVersion":2}),
                );
                self.command(
                    json!({"type":"set_subagent_subscription","id":"subagents","level":"progress"}),
                );
                self.command(json!({"type":"get_available_models","id":"models"}));
                self.command(json!({"type":"get_available_commands","id":"commands"}));
                self.command(json!({"type":"get_subagents","id":"agents"}));
                self.request_state();
            }
            "response" => {
                if self.on_model_control(&f) {
                    return;
                }
                if f["success"] == false {
                    let command = f["command"].as_str().unwrap_or("");
                    if ![
                        "negotiate_protocol",
                        "set_subagent_subscription",
                        "get_subagents",
                    ]
                    .contains(&command)
                    {
                        self.runtime.error =
                            Some(f["error"].as_str().unwrap_or("OMP 请求失败").into());
                    }
                    if command == "prompt" {
                        self.accept_submission(&f, window, cx);
                    }
                    if command == "abort_and_restore_queue" {
                        self.runtime.stopping = false;
                    }
                    if command == "promote_queued_message" {
                        self.runtime.steering_request = None;
                        self.request_state();
                    }
                    return;
                }
                match f["command"].as_str().unwrap_or("") {
                    "get_available_commands" => {
                        self.runtime.composer_state.commands = f["data"]["commands"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                    }
                    "promote_queued_message" => {
                        self.runtime.steering_request = None;
                        self.runtime.status = if f["data"]["promoted"] == true {
                            "指示已提交，接收时机由 OMP 决定"
                        } else {
                            "该指令已被模型接收"
                        }
                        .into();
                        self.request_state();
                    }
                    "get_state" => {
                        let d = &f["data"];
                        self.runtime.connected = true;
                        self.runtime.starting = false;
                        if d["isSettled"].is_boolean() && self.runtime.pending_send.is_none() {
                            self.runtime.running = d["isSettled"] == false;
                        }
                        self.runtime.queued_inputs = ["followUp"]
                            .into_iter()
                            .flat_map(|key| {
                                d["queuedMessages"][key].as_array().into_iter().flatten()
                            })
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect();
                        self.runtime.steered_inputs = d["queuedMessages"]["steering"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect();
                        self.runtime.status = if self.runtime.running {
                            self.runtime
                                .active_tools
                                .values()
                                .next()
                                .map(|name| format!("正在执行 {name}"))
                                .unwrap_or("OMP 正在思考…".into())
                        } else if self.runtime.interrupted_run {
                            "已停止".into()
                        } else {
                            "本地 OMP 已连接".into()
                        };
                        self.runtime.model_controls.label =
                            d["model"]["id"].as_str().unwrap_or("未配置模型").into();
                        let model_changed =
                            super::model_controls::model_key(&self.runtime.model_controls.current)
                                != super::model_controls::model_key(&d["model"]);
                        self.runtime.model_controls.current = d["model"].clone();
                        if model_changed {
                            self.request_thinking_levels();
                            self.request_thinking_source();
                        }
                        self.runtime.model_controls.thinking =
                            d["thinkingLevel"].as_str().unwrap_or("自动").into();
                        self.runtime.context_percent =
                            d["contextUsage"]["percent"].as_f64().unwrap_or(0.);
                        self.runtime.context_used = d["contextUsage"]["tokens"]
                            .as_f64()
                            .filter(|tokens| tokens.is_finite() && *tokens >= 0.)
                            .map(|tokens| tokens.round() as u64);
                        self.runtime.context_limit = d["contextUsage"]["contextWindow"]
                            .as_u64()
                            .or_else(|| d["model"]["contextWindow"].as_u64())
                            .unwrap_or(0);
                        self.runtime.todos = crate::domain::activity::merge_todo_snapshot(
                            &self.runtime.todos,
                            d["todoPhases"]
                                .as_array()
                                .map(Vec::as_slice)
                                .unwrap_or_default(),
                        );
                        self.save_activity();
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
                        if let Some(native_id) = d["sessionId"].as_str()
                            && let Some(s) = self.session_mut()
                            && s.native_id.as_deref() != Some(native_id)
                        {
                            s.native_id = Some(native_id.into());
                            self.dirty = true;
                        }
                    }
                    "get_available_models" => {
                        self.runtime.model_controls.available = f["data"]["models"]
                            .as_array()
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    "get_subagents" => {
                        let agents = f["data"]["subagents"]
                            .as_array()
                            .or_else(|| f["data"]["agents"].as_array())
                            .or_else(|| f["data"].as_array())
                            .cloned()
                            .unwrap_or_default();
                        for agent in agents {
                            self.on_subagent(&agent);
                        }
                    }
                    "set_todos" => {
                        self.runtime.todos = f["data"]["todoPhases"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        self.save_activity();
                    }
                    "prompt" => {
                        self.accept_submission(&f, window, cx);
                        if f["data"]["agentInvoked"] == false {
                            self.request_thinking_source();
                        }
                    }
                    "abort_and_restore_queue" => self.restore_queue(&f, window, cx),
                    "set_model" | "set_thinking_level" => {
                        self.request_state();
                        self.request_thinking_source();
                    }
                    _ => {}
                }
            }
            "available_commands_update" => {
                self.runtime.composer_state.commands =
                    f["commands"].as_array().cloned().unwrap_or_default();
            }
            "agent_start" => {
                self.runtime.running = true;
                self.runtime.status = "OMP 正在处理…".into();
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
                self.runtime.queued_inputs.clear();
                self.runtime.steered_inputs.clear();
                self.runtime.running = false;
                self.runtime.active_tools.clear();
                self.runtime.interrupted_run |= f["status"] == "aborted";
                if self.runtime.interrupted_run {
                    let start = self.runtime.run_message_start;
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
                self.runtime.status = if self.runtime.interrupted_run {
                    "已停止"
                } else {
                    "本轮已完成"
                }
                .into();
                if f["status"] == "error" || f["status"] == "aborted" {
                    self.runtime.status = "本轮已停止".into();
                    if let Some(e) = f["error"].as_str() {
                        self.runtime.error = Some(e.into());
                    }
                }
                self.request_state();
                self.refresh_files(cx);
                self.persist();
            }
            "message_start" | "message_update" | "message_end" => {
                self.runtime.text_update_pending = true;
                let m = &f["message"];
                if m["role"] == "user" {
                    self.queue_delivered(&crate::infrastructure::omp::message_text(m));
                }
                if m["role"] != "assistant" {
                    if let Some(mut message) = crate::infrastructure::omp::display_message(m)
                        && message.role != "user"
                    {
                        let rid = f["messageId"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| {
                                format!("{}:{}:{}", m["role"], m["customType"], m["timestamp"])
                            });
                        let mid = self
                            .runtime
                            .rpc_messages
                            .entry(rid)
                            .or_insert_with(id)
                            .clone();
                        message.id = mid.clone();
                        if let Some(s) = self.session_mut() {
                            if let Some(existing) = s.messages.iter_mut().find(|m| m.id == mid) {
                                existing.text = message.text;
                                existing.label = message.label;
                                existing.details = message.details;
                            } else {
                                s.messages.push(message);
                            }
                        }
                        self.dirty = true;
                    }
                    return;
                }
                let text = crate::infrastructure::omp::message_text(m);
                let mut process = process_blocks(
                    m,
                    kind,
                    f["assistantMessageEvent"]["type"].as_str().unwrap_or(""),
                );
                if text.is_empty() && process.is_empty() {
                    return;
                }
                let rid = f["messageId"].as_str().unwrap_or("stream").to_string();
                let mid = self
                    .runtime
                    .rpc_messages
                    .entry(rid)
                    .or_insert_with(id)
                    .clone();
                let linked = self.runtime.in_flight_comments.clone();
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
                self.runtime
                    .active_tools
                    .insert(call_id.clone(), name.into());
                let start = self.runtime.run_message_start;
                if let Some(s) = self.session_mut() {
                    s.start_tool(&call_id, name, &f["args"], start);
                }
                self.runtime.text_update_pending = true;
            }
            "tool_execution_end" => {
                let call_id = f["toolCallId"].as_str().unwrap_or_default();
                self.runtime.active_tools.remove(call_id);
                self.update_tool_activity(&f["result"]["details"]);
                if let Some(s) = self.session_mut() {
                    for block in s
                        .messages
                        .iter_mut()
                        .flat_map(|m| &mut m.process)
                        .filter(|b| b.kind == ProcessKind::Tool && b.id == call_id)
                    {
                        block.set_result(
                            crate::infrastructure::omp::message_text(&f["result"]),
                            &f["result"]["details"],
                        );
                    }
                }
                self.dirty = true;
                self.refresh_transcript();
                self.request_state();
            }
            "tool_execution_update" => {
                let partial = f
                    .get("partialResult")
                    .or_else(|| f.get("result"))
                    .unwrap_or(&Value::Null);
                self.update_tool_activity(&partial["details"]);
                let call = f["toolCallId"].as_str().unwrap_or_default();
                if let Some(s) = self.session_mut() {
                    for block in s
                        .messages
                        .iter_mut()
                        .flat_map(|m| &mut m.process)
                        .filter(|b| b.id == call)
                    {
                        block.set_result(
                            crate::infrastructure::omp::message_text(partial),
                            &partial["details"],
                        );
                    }
                }
                self.dirty = true;
            }
            "subagent_lifecycle" | "subagent_progress" => {
                self.on_subagent(&f["payload"]);
                if kind == "subagent_lifecycle" {
                    self.command(json!({"type":"get_subagents","id":"agents"}));
                }
            }
            "extension_ui_request" => match f["method"].as_str().unwrap_or("") {
                "select" | "confirm" | "input" | "editor" => {
                    if f["secret"] == true {
                        self.command(
                            json!({"type":"extension_ui_response","id":f["id"],"cancelled":true}),
                        );
                        self.runtime.error = Some("请在 OMP 终端完成敏感凭据输入后重新连接".into());
                    } else {
                        self.runtime.requests.push_back(f);
                    }
                }
                "cancel" => {
                    self.runtime.requests.retain(|r| r["id"] != f["targetId"]);
                }
                "notify" => {
                    self.runtime.status = f["message"].as_str().unwrap_or("").into();
                }
                "set_editor_text" => {
                    if let Some(text) = f["text"].as_str() {
                        self.runtime
                            .composer_state
                            .input
                            .update(cx, |i, cx| i.set_value(text.to_string(), window, cx));
                        if let Some(s) = self.session_mut() {
                            s.draft = text.into();
                        }
                        self.dirty = true;
                    }
                }
                _ => {}
            },
            "notice" | "extension_error" | "rpc_frame_error" => {
                self.runtime.error = Some(
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

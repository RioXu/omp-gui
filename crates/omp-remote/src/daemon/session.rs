use super::{Attach, Duration, Instant, UnixStream, mpsc};
use anyhow::{Context, Result};
use omp_core::{
    infrastructure::{
        omp::{Client, Event, SessionConnection, encode_transport_frame},
        persistence::workspace_store::data_dir,
    },
    remote::receipts::{Admission, Commit, Ledger, native_mutation},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    sync::Arc,
};

pub enum Input {
    Attach {
        request: Attach,
        generation: String,
        stream: UnixStream,
    },
    Frame {
        generation: String,
        frame: Value,
    },
    Detach(String),
}
type Outbound = (Arc<Vec<u8>>, Option<mpsc::Sender<()>>);
struct Subscriber {
    owner: String,
    generation: String,
    tx: mpsc::SyncSender<Outbound>,
    stream: UnixStream,
}
impl Drop for Subscriber {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
struct Actor {
    epoch: String,
    sequence: u64,
    ring: VecDeque<(u64, Arc<Vec<u8>>)>,
    bytes: usize,
    subscriber: Option<Subscriber>,
    ready: bool,
    busy: bool,
    idle: Instant,
    requests: HashMap<String, Value>,
    pending: HashMap<String, (String, Commit)>,
    messages: HashMap<String, String>,
    ledger: Ledger,
}

pub fn run(start: Attach, rx: mpsc::Receiver<Input>, key: &str) -> Result<()> {
    // Linux ties PDEATHSIG to the creating thread. Spawn on this durable actor,
    // never the SSH reader thread that intentionally exits during detachment.
    let client = Client::start_project_with(
        &start.omp,
        std::path::Path::new(&start.root),
        &start.resume,
        |command| {
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::process::CommandExt;
                let owner = unsafe { libc::getpid() };
                unsafe {
                    command.pre_exec(move || {
                        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        if libc::getppid() != owner {
                            return Err(std::io::Error::from_raw_os_error(libc::ECHILD));
                        }
                        Ok(())
                    });
                }
            }
            #[cfg(not(target_os = "linux"))]
            let _ = command;
        },
    );
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            if let Ok(Input::Attach { mut stream, .. }) = rx.recv_timeout(Duration::from_secs(5)) {
                let _=stream.write_all(&encode_transport_frame(&json!({"type":"__omp_gui_transport","state":"blocked","error":format!("{error:#}")}))?);
            }
            return Err(error);
        }
    };
    let mut actor = Actor {
        epoch: omp_core::domain::id(),
        sequence: 0,
        ring: VecDeque::new(),
        bytes: 0,
        subscriber: None,
        ready: false,
        busy: false,
        idle: Instant::now(),
        requests: HashMap::new(),
        pending: HashMap::new(),
        messages: HashMap::new(),
        ledger: Ledger::new(&data_dir(), &format!("native-{key}"))?,
    };
    loop {
        for event in client.poll(200) {
            match event {
                Event::Frame(mut frame) => {
                    match frame["type"].as_str().unwrap_or("") {
                        "ready" => actor.ready = true,
                        "agent_start" => actor.busy = true,
                        "session_settled" => actor.busy = false,
                        "prompt_result" if frame["sessionSettled"] != false => actor.busy = false,
                        "extension_ui_request" => {
                            let id = frame["id"].as_str().unwrap_or_default().to_owned();
                            if matches!(
                                frame["method"].as_str(),
                                Some("select" | "confirm" | "input" | "editor")
                            ) {
                                actor.requests.insert(id, frame.clone());
                            }
                            if frame["method"] == "cancel" {
                                actor
                                    .requests
                                    .remove(frame["targetId"].as_str().unwrap_or_default());
                            }
                        }
                        "response" => {
                            if frame["command"] == "prompt" && frame["success"] == false {
                                actor.busy = false;
                            }
                            if frame["command"] == "get_state"
                                && let Some(settled) = frame["data"]["isSettled"].as_bool()
                            {
                                actor.busy = !settled;
                            }
                            if let Some((_, commit)) = actor
                                .pending
                                .remove(frame["id"].as_str().unwrap_or_default())
                                && let Err(error) = commit.complete(Ok(frame.clone()))
                            {
                                actor.control(json!({"type":"notice","message":format!("操作已完成，但远端确认记录写入失败：{error:#}")}))?;
                            }
                            if frame["command"] == "get_messages"
                                && let Some(messages) = frame["data"]["messages"].as_array_mut()
                            {
                                for message in messages {
                                    if let Some(id) = actor.messages.get(&message_key(message)) {
                                        message["__omp_gui_message_id"] = json!(id);
                                    }
                                }
                            }
                        }
                        "message_start" | "message_update" | "message_end" => {
                            if let Some(id) = frame["messageId"].as_str() {
                                actor
                                    .messages
                                    .insert(message_key(&frame["message"]), id.into());
                            }
                        }
                        _ => {}
                    }
                    actor.publish(frame)?;
                }
                Event::Error(error) => actor.publish(json!({"type":"notice","message":error}))?,
                Event::Closed => {
                    actor.terminal(json!({"type":"__omp_gui_transport","state":"failed","error":"远端 OMP 已退出；运行中的工具可能已中断"}))?;
                    return Ok(());
                }
            }
        }
        for input in rx.try_iter().take(200) {
            match input {
                Input::Attach {
                    request,
                    generation,
                    mut stream,
                } => {
                    if actor
                        .subscriber
                        .as_ref()
                        .is_some_and(|subscriber| subscriber.owner != request.owner)
                    {
                        stream.write_all(&encode_transport_frame(&json!({"type":"__omp_gui_transport","state":"blocked","error":"此原生会话已在另一个 GUI 中连接"}))?)?;
                        continue;
                    }
                    actor.subscriber = None;
                    let (tx, events) = mpsc::sync_channel::<Outbound>(256);
                    let mut writer = stream.try_clone()?;
                    std::thread::spawn(move || {
                        for (bytes, ack) in events {
                            if writer.write_all(&bytes).is_err() {
                                break;
                            }
                            if let Some(ack) = ack {
                                let _ = ack.send(());
                            }
                        }
                        let _ = writer.shutdown(std::net::Shutdown::Both);
                    });
                    actor.subscriber = Some(Subscriber {
                        owner: request.owner,
                        generation,
                        tx,
                        stream,
                    });
                    let changed = !request.epoch.is_empty() && request.epoch != actor.epoch;
                    let after = if changed { 0 } else { request.after };
                    let backlog = actor
                        .ring
                        .iter()
                        .filter(|(sequence, _)| *sequence > after)
                        .count();
                    let gap = changed
                        || backlog > 128
                        || after > actor.sequence
                        || actor.ring.front().is_some_and(|(seq, _)| after + 1 < *seq);
                    actor.control(json!({"type":"__omp_gui_transport","state":"connected","epoch":actor.epoch,"gap":gap,"epoch_changed":changed,"requests":actor.requests.keys().collect::<Vec<_>>()}))?;
                    let replay = actor
                        .ring
                        .iter()
                        .filter(|(seq, _)| !gap && *seq > after)
                        .map(|(_, bytes)| bytes.clone())
                        .collect::<Vec<_>>();
                    for bytes in replay {
                        actor.deliver(bytes);
                    }
                    if actor.ready {
                        actor.control(json!({"type":"ready"}))?;
                    }
                    let requests = actor.requests.values().cloned().collect::<Vec<_>>();
                    for request in requests {
                        actor.control(request)?;
                    }
                }
                Input::Detach(generation) => {
                    if actor
                        .subscriber
                        .as_ref()
                        .is_some_and(|s| s.generation == generation)
                    {
                        actor.subscriber = None;
                        actor.idle = Instant::now();
                    }
                }
                Input::Frame {
                    generation,
                    mut frame,
                } => {
                    if !actor
                        .subscriber
                        .as_ref()
                        .is_some_and(|s| s.generation == generation)
                    {
                        continue;
                    }
                    let kind = frame["type"].as_str().unwrap_or_default().to_owned();
                    if kind == "__omp_gui_release_idle" {
                        // Only the current subscriber generation can reach here.
                        // Never infer ownership from a journal path or process ID.
                        if !actor.ready
                            || actor.busy
                            || !actor.requests.is_empty()
                            || !actor.pending.is_empty()
                        {
                            actor.control(json!({"type":"__omp_gui_released","id":frame["id"],"success":false,"error":"会话正在运行、等待交互或仍有未完成操作，未释放占用"}))?;
                            continue;
                        }
                        // Client::drop kills and waits for exactly our child.
                        // Acknowledge only after its native lease is released.
                        drop(client);
                        actor.terminal(
                            json!({"type":"__omp_gui_released","id":frame["id"],"success":true}),
                        )?;
                        return Ok(());
                    }
                    if kind == "__omp_gui_query" {
                        let token = frame["token"].as_str().unwrap_or_default();
                        match actor.ledger.query(token)? {
                            Some(receipt) if receipt.result.is_some()=>actor.control(receipt.result.unwrap().map_err(anyhow::Error::msg)?)?,
                            _ if actor.pending.values().any(|(pending,_)|pending==token)=>{},
                            _=>actor.control(json!({"type":"__omp_gui_operation","id":frame["id"],"token":token,"state":"unconfirmed"}))?,
                        }
                        continue;
                    }
                    let token = frame
                        .as_object_mut()
                        .context("无效 OMP 指令")?
                        .remove("__omp_gui_token")
                        .and_then(|value| value.as_str().map(str::to_owned));
                    if native_mutation(&kind) {
                        let token = token.context("变更指令缺少去重编号")?;
                        let digest = omp_core::domain::hash(&serde_json::to_string(&frame)?);
                        let admission = match actor.ledger.begin(&token, digest) {
                            Ok(admission) => admission,
                            Err(error) => {
                                actor.control(json!({"type":"response","id":frame["id"],"command":kind,"success":false,"error":format!("{error:#}")}))?;
                                continue;
                            }
                        };
                        match admission {
                            Admission::New(commit) => {
                                // UI responses have no native response; admission still prevents replay.
                                if kind == "extension_ui_response" {
                                    let request =
                                        frame["id"].as_str().unwrap_or_default().to_owned();
                                    client.send(frame)?;
                                    actor.requests.remove(&request);
                                    let acknowledgement = json!({"type":"__omp_gui_operation","token":token,"state":"forwarded"});
                                    if let Err(error) = commit.complete(Ok(acknowledgement.clone()))
                                    {
                                        actor.control(json!({"type":"notice","message":format!("交互答复已转发，但远端确认记录写入失败：{error:#}")}))?;
                                    }
                                    actor.control(acknowledgement)?;
                                    continue;
                                }
                                actor.pending.insert(
                                    frame["id"].as_str().context("变更指令缺少 id")?.into(),
                                    (token, commit),
                                );
                            }
                            Admission::Existing(Some(receipt)) if receipt.result.is_some() => {
                                actor.control(
                                    receipt.result.unwrap().map_err(anyhow::Error::msg)?,
                                )?;
                                continue;
                            }
                            Admission::Existing(_) => {
                                actor.control(json!({"type":"__omp_gui_operation","id":frame["id"],"token":token,"state":"unconfirmed"}))?;
                                continue;
                            }
                        }
                    }
                    if kind == "prompt" {
                        actor.busy = true;
                    }
                    client.send(frame)?;
                }
            }
        }
        if actor.subscriber.is_some() || actor.busy || !actor.requests.is_empty() {
            actor.idle = Instant::now();
        }
        if actor.subscriber.is_none()
            && !actor.busy
            && actor.requests.is_empty()
            && actor.idle.elapsed() > Duration::from_secs(10)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn message_key(message: &Value) -> String {
    format!(
        "{}:{}:{}",
        message["role"], message["customType"], message["timestamp"]
    )
}
impl Actor {
    fn deliver(&mut self, bytes: Arc<Vec<u8>>) {
        if self
            .subscriber
            .as_ref()
            .is_some_and(|subscriber| subscriber.tx.try_send((bytes, None)).is_err())
        {
            self.subscriber = None;
            self.idle = Instant::now();
        }
    }
    fn control(&mut self, frame: Value) -> Result<()> {
        self.deliver(Arc::new(encode_transport_frame(&frame)?));
        Ok(())
    }
    fn terminal(&mut self, frame: Value) -> Result<()> {
        if let Some(subscriber) = &self.subscriber {
            let (tx, rx) = mpsc::channel();
            if subscriber
                .tx
                .try_send((Arc::new(encode_transport_frame(&frame)?), Some(tx)))
                .is_ok()
            {
                let _ = rx.recv_timeout(Duration::from_secs(2));
            }
        }
        Ok(())
    }
    fn publish(&mut self, mut frame: Value) -> Result<()> {
        self.sequence += 1;
        frame["__omp_gui"] = json!({"epoch":self.epoch,"sequence":self.sequence});
        let bytes = Arc::new(encode_transport_frame(&frame)?);
        self.bytes += bytes.len();
        self.ring.push_back((self.sequence, bytes.clone()));
        while self.bytes > 64 * 1024 * 1024 || self.ring.len() > 8192 {
            if let Some((_, old)) = self.ring.pop_front() {
                self.bytes -= old.len();
            }
        }
        self.deliver(bytes);
        Ok(())
    }
}

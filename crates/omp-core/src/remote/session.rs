//! Reconnect only the event bridge; never replay a mutating native command.
use super::{RemoteHost, receipts::native_mutation};
use crate::{
    environment::HostPath,
    infrastructure::omp::{Client, Event, SessionConnection},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

pub struct RemoteConnection {
    host: RemoteHost,
    omp: String,
    root: HostPath,
    resume: String,
    owner: String,
    state: Mutex<State>,
}
struct State {
    client: Option<Client>,
    after: u64,
    epoch: String,
    online: bool,
    ready: bool,
    fatal: bool,
    pending: HashMap<String, (String, String)>,
    reconnect: Option<mpsc::Receiver<Result<Client, String>>>,
    next: Instant,
    attempt: u32,
}
impl RemoteConnection {
    #[cfg(feature = "test-support")]
    pub fn interrupt_transport(&self) {
        if let Ok(mut state) = self.state.lock()
            && let Some(client) = state.client.as_mut()
        {
            client.interrupt_transport();
        }
    }
    pub fn start(host: RemoteHost, omp: &str, root: &HostPath, resume: &str) -> Result<Self> {
        let owner = crate::domain::id();
        let client = host.start_bridge(omp, root, resume, &owner, 0, "")?;
        Ok(Self {
            host,
            omp: omp.into(),
            root: root.clone(),
            resume: resume.into(),
            owner,
            state: Mutex::new(State {
                client: Some(client),
                after: 0,
                epoch: String::new(),
                online: true,
                ready: false,
                fatal: false,
                pending: HashMap::new(),
                reconnect: None,
                next: Instant::now(),
                attempt: 0,
            }),
        })
    }
}
impl SessionConnection for RemoteConnection {
    fn release_idle(self: Box<Self>) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("远端连接状态不可用"))?;
        ensure!(
            state.online && !state.fatal && state.reconnect.is_none(),
            "远端连接尚未恢复，无法确认占用释放"
        );
        let client = state
            .client
            .take()
            .context("远端连接已断开，无法确认占用释放")?;
        let id = crate::domain::id();
        let mut requested = state.ready;
        if requested {
            client.send(json!({"type":"__omp_gui_release_idle","id":id}))?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for event in client.poll(200) {
                match event {
                    Event::Frame(frame) if frame["type"] == "ready" && !requested => {
                        client.send(json!({"type":"__omp_gui_release_idle","id":id}))?;
                        requested = true;
                    }
                    Event::Frame(frame)
                        if frame["type"] == "__omp_gui_released" && frame["id"] == id =>
                    {
                        ensure!(
                            frame["success"] == true,
                            "{}",
                            frame["error"]
                                .as_str()
                                .unwrap_or("远端会话尚未空闲，未释放占用")
                        );
                        return Ok(());
                    }
                    Event::Closed => anyhow::bail!("连接已关闭，占用释放结果未确认"),
                    Event::Error(error) => anyhow::bail!("占用释放失败：{error}"),
                    _ => {}
                }
            }
            ensure!(Instant::now() < deadline, "远端占用释放超时，已保留会话");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn send(&self, mut frame: Value) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("远端连接状态不可用"))?;
        ensure!(
            state.online && !state.fatal,
            "远端正在重连，输入已保留；不会自动重发"
        );
        let kind = frame["type"].as_str().unwrap_or_default().to_owned();
        let token = if native_mutation(&kind) {
            ensure!(
                state.pending.len() < 256,
                "远端有过多待确认操作，请等待结果"
            );
            let token = crate::domain::id();
            state.pending.insert(
                token.clone(),
                (frame["id"].as_str().unwrap_or_default().into(), kind),
            );
            frame["__omp_gui_token"] = json!(token);
            if frame["type"] != "extension_ui_response" {
                frame["id"] = json!(token);
            }
            Some(token)
        } else {
            None
        };
        let result = state.client.as_ref().context("远端正在重连")?.send(frame);
        if result.is_err()
            && let Some(token) = token
        {
            state.pending.remove(&token);
        }
        result
    }
    fn poll(&self, limit: usize) -> Vec<Event> {
        let Ok(mut state) = self.state.lock() else {
            return vec![Event::Error("远端连接状态不可用".into())];
        };
        let mut output = Vec::new();
        if let Some(result) = state.reconnect.as_ref().and_then(|rx| rx.try_recv().ok()) {
            state.reconnect = None;
            match result {
                Ok(client) => state.client = Some(client),
                Err(_) => {
                    state.attempt += 1;
                    state.next = Instant::now()
                        + Duration::from_secs((1u64 << state.attempt.min(4)).min(15));
                }
            }
        }
        let events = state
            .client
            .as_ref()
            .map(|client| client.poll(limit))
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Frame(mut frame) => {
                    if frame["type"] == "ready" {
                        state.ready = true;
                    }
                    if frame["type"] == "__omp_gui_transport" {
                        match frame["state"].as_str() {
                            Some("connected") => {
                                state.online = true;
                                state.attempt = 0;
                                let epoch = frame["epoch"].as_str().unwrap_or_default();
                                if state.epoch != epoch {
                                    state.after = 0;
                                    state.epoch = epoch.into();
                                }
                                for (token, (id, _)) in &state.pending {
                                    if let Some(client) = &state.client {
                                        let _ = client.send(
                                            json!({"type":"__omp_gui_query","token":token,"id":id}),
                                        );
                                    }
                                }
                            }
                            Some("blocked" | "failed") => {
                                state.fatal = true;
                                state.online = false;
                            }
                            _ => {}
                        }
                    }
                    if let Some(sequence) = frame["__omp_gui"]["sequence"].as_u64() {
                        if sequence <= state.after {
                            continue;
                        }
                        state.after = sequence;
                    }
                    if frame["type"] == "response" {
                        let token = frame["id"].as_str().unwrap_or_default();
                        if let Some((id, _)) = state.pending.remove(token) {
                            frame["id"] = json!(id);
                        }
                    }
                    if frame["type"] == "__omp_gui_operation" {
                        if let Some((id, kind)) = state
                            .pending
                            .remove(frame["token"].as_str().unwrap_or_default())
                            && frame["state"] == "unconfirmed"
                        {
                            output.push(Event::Frame(json!({"type":"response","id":id,"command":kind,"success":false,"error":"操作结果待确认；请先核对远端会话历史，不会自动重发"})));
                        }
                        continue;
                    }
                    output.push(Event::Frame(frame));
                }
                Event::Closed => {
                    state.client = None;
                    state.online = false;
                    state.ready = false;
                    state.attempt += 1;
                    state.next = Instant::now()
                        + Duration::from_millis((500u64 << state.attempt.min(5)).min(15000));
                    if state.fatal {
                        output.push(Event::Closed);
                    } else {
                        output.push(Event::Frame(
                            json!({"type":"__omp_gui_transport","state":"disconnected"}),
                        ));
                    }
                }
                Event::Error(error) => {
                    output.push(Event::Error(error));
                }
            }
        }
        if state.fatal && state.client.is_some() {
            state.client = None;
            output.push(Event::Closed);
        }
        if state.client.is_none()
            && state.reconnect.is_none()
            && !state.fatal
            && Instant::now() >= state.next
        {
            let host = self.host.clone();
            let omp = self.omp.clone();
            let root = self.root.clone();
            let resume = self.resume.clone();
            let owner = self.owner.clone();
            let after = state.after;
            let epoch = state.epoch.clone();
            let (tx, rx) = mpsc::channel();
            state.reconnect = Some(rx);
            std::thread::spawn(move || {
                let result = host
                    .start_bridge(&omp, &root, &resume, &owner, after, &epoch)
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send(result);
            });
        }
        output
    }
}

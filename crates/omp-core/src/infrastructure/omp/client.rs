//! Manage the OMP child process and asynchronous JSONL transport.
use super::framing::{Decoder, MAX_FRAME, MAX_MESSAGE};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Stdio},
    sync::mpsc,
    thread,
};

pub enum Event {
    Frame(Value),
    Error(String),
    Closed,
}
pub struct Client {
    child: Child,
    tx: mpsc::Sender<Value>,
    pub events: mpsc::Receiver<Event>,
}
impl Client {
    pub fn start(
        executable: &str,
        project: &Path,
        session_dir: &Path,
        resume: Option<&str>,
    ) -> Result<Self> {
        std::fs::create_dir_all(session_dir)?;
        Self::launch(executable, project, Some(session_dir), resume)
    }
    /// Use the same default project store as terminal OMP.
    pub fn start_project(executable: &str, project: &Path, resume: &str) -> Result<Self> {
        Self::launch(executable, project, None, Some(resume))
    }
    fn launch(
        executable: &str,
        project: &Path,
        session_dir: Option<&Path>,
        resume: Option<&str>,
    ) -> Result<Self> {
        let project = super::sessions::plain_path(project);
        let resolved = super::installation::resolve(executable)?;
        let mut cmd = crate::infrastructure::process::hidden_command(&resolved.to_string_lossy());
        cmd.args(["--mode", "rpc-ui", "--no-title", "--cwd"])
            .arg(&project)
            .current_dir(&project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = session_dir {
            cmd.arg("--session-dir").arg(dir);
        }
        if let Some(path) = resume {
            cmd.arg("--resume").arg(path);
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("无法启动 {executable}，请检查设置中的 OMP 路径"))?;
        let mut stdin = child.stdin.take().context("OMP stdin missing")?;
        let stdout = child.stdout.take().context("OMP stdout missing")?;
        let stderr = child.stderr.take().context("OMP stderr missing")?;
        let (tx, rx) = mpsc::channel::<Value>();
        let (event_tx, events) = mpsc::channel();
        let write_tx = event_tx.clone();
        thread::spawn(move || {
            for frame in rx {
                let result = (|| -> Result<()> {
                    let mut bytes = serde_json::to_vec(&frame)?;
                    bytes.push(b'\n');
                    ensure!(
                        bytes.len() <= MAX_MESSAGE,
                        "消息及附件超过 64 MiB，请减少附件"
                    );
                    stdin.write_all(&bytes)?;
                    stdin.flush()?;
                    Ok(())
                })();
                if let Err(e) = result {
                    let _ = write_tx.send(Event::Error(format!("发送失败：{e:#}")));
                    break;
                }
            }
        });
        let read_tx = event_tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut decoder = Decoder::default();
            loop {
                let mut line = Vec::new();
                let result = reader
                    .by_ref()
                    .take((MAX_FRAME + 1) as u64)
                    .read_until(b'\n', &mut line);
                match result {
                    Ok(0) => break,
                    Ok(_) => match decoder.decode(&line) {
                        Ok(Some(frame)) => {
                            if read_tx.send(Event::Frame(frame)).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = read_tx.send(Event::Error(format!("OMP 通信错误：{e:#}")));
                            break;
                        }
                    },
                    Err(e) => {
                        let _ = read_tx.send(Event::Error(e.to_string()));
                        break;
                    }
                }
            }
            let _ = read_tx.send(Event::Closed);
        });
        // Drain stderr to avoid deadlocking the child. Surface a bounded tail on failures.
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("Error") || line.contains("error:") {
                    let msg: String = line.chars().take(1000).collect();
                    let _ = event_tx.send(Event::Error(msg));
                }
            }
        });
        Ok(Self { child, tx, events })
    }
    pub fn send(&self, frame: Value) -> Result<()> {
        // Native rpc-input parses complete JSONL commands (including images).
        // MAX_FRAME applies to received physical frames; outgoing commands are
        // bounded by the logical message limit instead.
        if serde_json::to_vec(&frame)?.len() + 1 > MAX_MESSAGE {
            bail!("消息及附件超过 64 MiB，请减少附件后重试");
        }
        self.tx.send(frame).context("OMP 连接已关闭")
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Stdio},
    sync::mpsc,
    thread,
};

const MAX_FRAME: usize = 1024 * 1024;
#[cfg(test)]
mod live_test;
const MAX_MESSAGE: usize = 64 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct Decoder {
    chunk: Option<Chunk>,
}
struct Chunk {
    id: String,
    count: usize,
    next: usize,
    bytes: usize,
    data: Vec<u8>,
}
impl Decoder {
    pub fn decode(&mut self, line: &[u8]) -> Result<Option<Value>> {
        ensure!(line.len() <= MAX_FRAME, "OMP frame exceeds limit");
        let value: Value = serde_json::from_slice(line)?;
        if value["type"] != "rpc_chunk" {
            ensure!(self.chunk.is_none(), "interrupted chunk sequence");
            return Ok(Some(value));
        }
        let id = value["chunkId"]
            .as_str()
            .context("missing chunk id")?
            .to_string();
        let ix = value["index"].as_u64().context("missing chunk index")? as usize;
        let count = value["count"].as_u64().context("missing chunk count")? as usize;
        let bytes = value["byteLength"]
            .as_u64()
            .context("missing chunk length")? as usize;
        ensure!(
            count > 0 && count <= 100000 && bytes <= MAX_MESSAGE,
            "invalid chunk limits"
        );
        if ix == 0 {
            ensure!(self.chunk.is_none(), "interleaved chunks");
            self.chunk = Some(Chunk {
                id: id.clone(),
                count,
                next: 0,
                bytes,
                data: Vec::new(),
            });
        }
        let chunk = self.chunk.as_mut().context("chunk without start")?;
        ensure!(
            chunk.id == id && chunk.next == ix && chunk.count == count && chunk.bytes == bytes,
            "out of order chunk"
        );
        chunk
            .data
            .extend(STANDARD.decode(value["data"].as_str().context("missing chunk payload")?)?);
        ensure!(chunk.data.len() <= bytes, "chunk exceeds declared length");
        chunk.next += 1;
        if chunk.next == count {
            let c = self.chunk.take().unwrap();
            ensure!(c.data.len() == bytes, "chunk length mismatch");
            return Ok(Some(serde_json::from_slice(&c.data)?));
        }
        Ok(None)
    }
}

pub(crate) enum Event {
    Frame(Value),
    Error(String),
    Closed,
}
pub(crate) struct Engine {
    child: Child,
    tx: mpsc::Sender<Value>,
    pub events: mpsc::Receiver<Event>,
}
impl Engine {
    pub fn start(
        executable: &str,
        project: &Path,
        session_dir: &Path,
        resume: Option<&str>,
    ) -> Result<Self> {
        std::fs::create_dir_all(session_dir)?;
        let mut cmd = crate::files::hidden_command(executable);
        cmd.args(["--mode", "rpc-ui", "--no-title", "--cwd"])
            .arg(project)
            .arg("--session-dir")
            .arg(session_dir)
            .current_dir(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
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
                        bytes.len() <= MAX_FRAME,
                        "发送内容超过 OMP 单帧限制，请缩短指令"
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
        if serde_json::to_vec(&frame)?.len() + 1 > MAX_FRAME {
            bail!("消息过长，请缩短后重试");
        }
        self.tx.send(frame).context("OMP 连接已关闭")
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn message_text(message: &Value) -> String {
    if let Some(s) = message["content"].as_str() {
        return s.into();
    }
    message["content"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunked_unicode_roundtrip() {
        let bytes = serde_json::to_vec(&json!({"type":"message_end","text":"你好 🌍"})).unwrap();
        let split = bytes.len() / 2;
        let mut d = Decoder::default();
        for (i, part) in [&bytes[..split], &bytes[split..]].iter().enumerate() {
            let frame = json!({"type":"rpc_chunk","chunkId":"a","count":2,"index":i,"byteLength":bytes.len(),"data":STANDARD.encode(part)});
            let v = d.decode(&serde_json::to_vec(&frame).unwrap()).unwrap();
            if i == 0 {
                assert!(v.is_none());
            } else {
                assert_eq!(v.unwrap()["text"], "你好 🌍");
            }
        }
    }
    #[test]
    fn rejects_interrupted_and_out_of_order_chunks() {
        let mut d = Decoder::default();
        let f = json!({"type":"rpc_chunk","chunkId":"a","count":2,"index":0,"byteLength":4,"data":STANDARD.encode(b"{}")});
        d.decode(&serde_json::to_vec(&f).unwrap()).unwrap();
        assert!(d.decode(br#"{"type":"ready"}"#).is_err());
        let mut d = Decoder::default();
        let mut f = f;
        f["index"] = json!(1);
        assert!(d.decode(&serde_json::to_vec(&f).unwrap()).is_err());
    }
    #[test]
    fn extracts_only_public_text() {
        assert_eq!(
            message_text(
                &json!({"content":[{"type":"thinking","thinking":"private"},{"type":"text","text":"reply"}]})
            ),
            "reply"
        );
    }
}

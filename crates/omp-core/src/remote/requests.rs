//! Bounded management requests, isolated from model events and attachment streams.
use super::{protocol::*, ssh::ssh_command};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    io::{BufReader, Read, Write},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(super) struct Failure {
    pub message: String,
    pub transport: bool,
}
pub(super) type Work = (Request, mpsc::Sender<Result<Value, Failure>>, Instant);
struct Transport {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    tail: Arc<Mutex<String>>,
    deadline: Arc<Mutex<Option<Instant>>>,
    done: Arc<AtomicBool>,
}
impl Transport {
    fn open(host: &str, helper: &str, profile: Option<&str>) -> Result<Self> {
        let mut args = vec!["--stdio"];
        if let Some(profile) = profile {
            args.extend(["--profile", profile]);
        }
        let mut child = ssh_command(host, helper, &args)?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("无法启动 SSH，请检查客户端及连接配置")?;
        let input = child.stdin.take().context("SSH stdin missing")?;
        let output = BufReader::new(child.stdout.take().context("SSH stdout missing")?);
        let mut stderr = child.stderr.take().context("SSH stderr missing")?;
        let tail = Arc::new(Mutex::new(String::new()));
        let diagnostics = tail.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            while let Ok(n) = stderr.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                if let Ok(mut text) = diagnostics.lock() {
                    text.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    if text.len() > 4096 {
                        let mut start = text.len() - 4096;
                        while !text.is_char_boundary(start) {
                            start += 1;
                        }
                        *text = text[start..].into();
                    }
                }
            }
        });
        let child = Arc::new(Mutex::new(child));
        let watched = child.clone();
        let deadline = Arc::new(Mutex::new(None::<Instant>));
        let clock = deadline.clone();
        let done = Arc::new(AtomicBool::new(false));
        let stopped = done.clone();
        std::thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                if clock
                    .lock()
                    .ok()
                    .and_then(|deadline| *deadline)
                    .is_some_and(|deadline| Instant::now() > deadline)
                {
                    if let Ok(mut child) = watched.lock() {
                        let _ = child.kill();
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        Ok(Self {
            child,
            input,
            output,
            tail,
            deadline,
            done,
        })
    }
    fn request(&mut self, request: &Request, expires: Instant) -> Result<Value, Failure> {
        let mut transport = true;
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = Some(expires.min(Instant::now() + Duration::from_secs(40)));
        }
        let result = (|| -> Result<Value> {
            let bytes = serde_json::to_vec(request)?;
            ensure!(bytes.len() < MAX_MESSAGE, "远端请求超过 64 MiB");
            self.input.write_all(&bytes)?;
            self.input.write_all(b"\n")?;
            self.input.flush()?;
            let response =
                super::response::read(&mut self.output, &request.id).with_context(|| {
                    format!(
                        "SSH 响应读取失败：{}",
                        self.tail
                            .lock()
                            .map(|text| text.trim().to_owned())
                            .unwrap_or_default()
                    )
                })?;
            ensure!(
                response.protocol == VERSION && response.id == request.id,
                "远端响应身份或版本不匹配"
            );
            transport = false;
            response.result.map_err(anyhow::Error::msg)
        })();
        if let Ok(mut deadline) = self.deadline.lock() {
            *deadline = None;
        }
        result.map_err(|error| Failure {
            message: format!("{error:#}"),
            transport,
        })
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn start(
    host: &str,
    helper: &str,
    profile: Option<&str>,
) -> Result<mpsc::SyncSender<Work>> {
    let mut transport = Some(Transport::open(host, helper, profile)?);
    let host = host.to_owned();
    let helper = helper.to_owned();
    let profile = profile.map(str::to_owned);
    let (tx, rx) = mpsc::sync_channel::<Work>(32);
    std::thread::spawn(move || {
        for (request, reply, expires) in rx {
            if Instant::now() >= expires {
                let _ = reply.send(Err(Failure {
                    message: "请求在发送前已超时，未执行".into(),
                    transport: false,
                }));
                continue;
            }
            if transport.is_none() {
                match Transport::open(&host, &helper, profile.as_deref()) {
                    Ok(connection) => transport = Some(connection),
                    Err(error) => {
                        let _ = reply.send(Err(Failure {
                            message: format!("{error:#}"),
                            transport: true,
                        }));
                        continue;
                    }
                }
            }
            let result = transport.as_mut().unwrap().request(&request, expires);
            let failed = result.as_ref().err().is_some_and(|error| error.transport);
            let _ = reply.send(result);
            if failed {
                transport = None;
            }
        }
    });
    Ok(tx)
}

use super::ExecutionHost;
use crate::{environment::HostPath, remote::ssh::ssh_command};
use anyhow::{Context, Result};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, Stdio},
    sync::mpsc,
};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct FileChange {
    pub links_changed: bool,
}
pub trait FileWatch: Send {
    fn poll(&self) -> Vec<Result<FileChange, String>>;
}

struct LocalWatch {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<Result<FileChange, String>>,
}
impl FileWatch for LocalWatch {
    fn poll(&self) -> Vec<Result<FileChange, String>> {
        self.rx.try_iter().collect()
    }
}

struct RemoteWatch {
    child: Child,
    _stdin: std::process::ChildStdin,
    rx: mpsc::Receiver<Result<FileChange, String>>,
}
impl FileWatch for RemoteWatch {
    fn poll(&self) -> Vec<Result<FileChange, String>> {
        self.rx.try_iter().collect()
    }
}
impl Drop for RemoteWatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ExecutionHost {
    pub fn watch(&self, root: &HostPath, external: &[HostPath]) -> Result<Box<dyn FileWatch>> {
        match self {
            Self::Local => {
                let (tx, rx) = mpsc::channel();
                let watched_root = root.local()?.to_owned();
                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        let change = match event {
                            Ok(event)
                                if !matches!(event.kind, EventKind::Access(_))
                                    && event.paths.iter().any(|path| {
                                        path.strip_prefix(&watched_root).map_or(true, |relative| {
                                            !relative.components().any(|part| {
                                                matches!(
                                                    part.as_os_str().to_str(),
                                                    Some("target" | "node_modules" | ".venv")
                                                )
                                            })
                                        })
                                    }) =>
                            {
                                Some(Ok(FileChange {
                                    links_changed: matches!(
                                        event.kind,
                                        EventKind::Create(_)
                                            | EventKind::Remove(_)
                                            | EventKind::Modify(notify::event::ModifyKind::Name(_))
                                    ),
                                }))
                            }
                            Err(error) => Some(Err(error.to_string())),
                            _ => None,
                        };
                        if let Some(change) = change {
                            let _ = tx.send(change);
                        }
                    })?;
                watcher.watch(root.local()?, RecursiveMode::Recursive)?;
                for parent in external {
                    let _ = watcher.watch(parent.local()?, RecursiveMode::NonRecursive);
                }
                Ok(Box::new(LocalWatch {
                    _watcher: watcher,
                    rx,
                }))
            }
            Self::Remote(remote) => {
                let root = root.text();
                let external = external.iter().map(HostPath::text).collect::<Vec<_>>();
                let mut args = vec!["--watch", "--cwd", root.as_str()];
                if let Some(profile) = remote.profile.as_deref() {
                    args.extend(["--profile", profile]);
                }
                for parent in &external {
                    args.extend(["--external", parent]);
                }
                let mut child = ssh_command(&remote.host, &remote.helper, &args)?
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()?;
                let input = child.stdin.take().context("SSH watch stdin missing")?;
                let output = child.stdout.take().context("SSH watch stdout missing")?;
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let mut output = BufReader::new(output);
                    loop {
                        let mut line = Vec::new();
                        let result = (|| -> Result<Option<FileChange>> {
                            let count = output.by_ref().take(4097).read_until(b'\n', &mut line)?;
                            if count == 0 {
                                return Ok(None);
                            }
                            anyhow::ensure!(
                                count <= 4096 && line.ends_with(b"\n"),
                                "无效远端文件通知"
                            );
                            Ok(Some(serde_json::from_slice(&line)?))
                        })();
                        match result {
                            Ok(Some(change)) => {
                                if tx.send(Ok(change)).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => {
                                let _ = tx.send(Err("远端文件监听连接已断开".into()));
                                break;
                            }
                            Err(error) => {
                                let _ = tx.send(Err(format!("{error:#}")));
                                break;
                            }
                        }
                    }
                });
                Ok(Box::new(RemoteWatch {
                    child,
                    _stdin: input,
                    rx,
                }))
            }
        }
    }
}

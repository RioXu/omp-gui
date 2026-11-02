use super::ExecutionHost;
use crate::{environment::HostPath, remote::ssh::ssh_command};
use anyhow::{Context, Result};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    io::BufReader,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct FileChange {
    pub links_changed: bool,
    pub paths: Vec<String>,
    pub full_rescan: bool,
}
impl FileChange {
    pub fn merge(&mut self, other: Self) {
        self.links_changed |= other.links_changed;
        self.full_rescan |= other.full_rescan;
        self.paths.extend(other.paths);
        self.paths.sort();
        self.paths.dedup();
        if self.paths.len() > 512 {
            self.paths.clear();
            self.full_rescan = true;
        }
    }
    pub fn affects(&self, path: &HostPath) -> bool {
        self.full_rescan
            || self.paths.iter().any(|changed| match path {
                HostPath::Local(path) => path.starts_with(changed),
                HostPath::Posix(path) => {
                    path == changed
                        || path.starts_with(&format!("{}/", changed.trim_end_matches('/')))
                }
            })
    }
    pub fn git_changed(&self) -> bool {
        self.paths
            .iter()
            .any(|path| path.split(['/', '\\']).any(|p| p == ".git"))
    }
}
pub trait FileWatch: Send {
    fn poll(&self) -> Vec<Result<FileChange, String>>;
}

struct LocalWatch {
    _watcher: notify::RecommendedWatcher,
    overflow: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<FileChange, String>>,
}
impl FileWatch for LocalWatch {
    fn poll(&self) -> Vec<Result<FileChange, String>> {
        let mut change = FileChange::default();
        let mut seen = self.overflow.swap(false, Ordering::AcqRel);
        change.full_rescan = seen;
        for event in self.rx.try_iter() {
            match event {
                Ok(event) => {
                    seen = true;
                    change.merge(event);
                }
                Err(error) => return vec![Err(error)],
            }
        }
        if seen { vec![Ok(change)] } else { Vec::new() }
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

// Git metadata changes still refresh status, but cannot change a Markdown
// file reference. In particular, index.lock creation/removal is not a new
// project file and must not cause the conversation to be parsed again.
fn changes_file_links(root: &std::path::Path, event: &notify::Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(notify::event::ModifyKind::Name(_))
    ) && event.paths.iter().any(|path| {
        path.strip_prefix(root).map_or(true, |relative| {
            !relative.components().any(|part| part.as_os_str() == ".git")
        })
    })
}

impl ExecutionHost {
    pub fn watch(&self, root: &HostPath, external: &[HostPath]) -> Result<Box<dyn FileWatch>> {
        match self {
            Self::Local => {
                let (tx, rx) = mpsc::sync_channel(256);
                let overflow = Arc::new(AtomicBool::new(false));
                let lost = overflow.clone();
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
                                    links_changed: changes_file_links(&watched_root, &event),
                                    paths: event
                                        .paths
                                        .iter()
                                        .take(512)
                                        .map(|p| p.to_string_lossy().into_owned())
                                        .collect(),
                                    full_rescan: event.need_rescan() || event.paths.len() > 512,
                                }))
                            }
                            Err(error) => Some(Err(error.to_string())),
                            _ => None,
                        };
                        if let Some(change) = change
                            && tx.try_send(change).is_err()
                        {
                            lost.store(true, Ordering::Release);
                        }
                    })?;
                watcher.watch(root.local()?, RecursiveMode::Recursive)?;
                for parent in external {
                    let _ = watcher.watch(parent.local()?, RecursiveMode::NonRecursive);
                }
                Ok(Box::new(LocalWatch {
                    _watcher: watcher,
                    overflow,
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
                        let result = crate::remote::wire::read::<FileChange>(
                            &mut output,
                            crate::remote::protocol::MAX_MESSAGE,
                        );
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

#[cfg(test)]
mod tests {
    use super::changes_file_links;
    use notify::{
        Event, EventKind,
        event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
    };
    #[test]
    fn git_metadata_does_not_reparse_conversation_file_links() {
        let root = std::path::Path::new("project");
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Remove(RemoveKind::File),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        ] {
            for metadata in [".git/index.lock", ".git/index", ".git/refs/heads/main"] {
                assert!(!changes_file_links(
                    root,
                    &Event::new(kind).add_path(root.join(metadata))
                ));
            }
            assert!(changes_file_links(
                root,
                &Event::new(kind).add_path(root.join("src/新文件.rs"))
            ));
            // A real path involved in a rename still invalidates links.
            assert!(changes_file_links(
                root,
                &Event::new(kind)
                    .add_path(root.join(".git/tmp"))
                    .add_path(root.join("src/new.rs"))
            ));
        }
        assert!(!changes_file_links(
            root,
            &Event::new(EventKind::Modify(ModifyKind::Data(
                notify::event::DataChange::Any
            )))
            .add_path(root.join("src/existing.rs"))
        ));
    }
}

#[cfg(test)]
mod change_tests {
    use super::*;
    #[test]
    fn merging_bounds_paths_and_keeps_posix_backslashes_literal() {
        let mut change = FileChange {
            paths: vec![
                "/project/a".into(),
                "/project/a".into(),
                "/project/a\\b".into(),
            ],
            ..Default::default()
        };
        change.merge(FileChange {
            links_changed: true,
            paths: vec!["/project/a/child".into()],
            ..Default::default()
        });
        assert_eq!(change.paths.len(), 3);
        assert!(change.affects(&HostPath::Posix("/project/a/child/file".into())));
        assert!(!change.affects(&HostPath::Posix("/project/another".into())));
        assert!(change.affects(&HostPath::Posix("/project/a\\b".into())));
        change.merge(FileChange {
            paths: (0..513).map(|i| format!("/p/{i}")).collect(),
            ..Default::default()
        });
        assert!(change.full_rescan);
        assert!(change.paths.is_empty());
    }
}

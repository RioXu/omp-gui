use super::*;
use crate::{
    domain::Session,
    infrastructure::omp::{
        self,
        sessions::{NativeSession, Registration, Scan, SessionStore},
    },
    remote::{RemoteHost, protocol::Operation},
};
use anyhow::Context;
use std::sync::Arc;

/// A configured execution host; UI components consume its capability traits.
#[derive(Clone)]
pub enum ExecutionHost {
    Local,
    Remote(Arc<RemoteHost>),
}

impl ExecutionHost {
    pub fn child_transcript(
        &self,
        primary: &str,
        agent: &str,
        path: Option<&str>,
    ) -> Result<Vec<crate::domain::Message>> {
        match self {
            Self::Local => {
                SessionStore::child_transcript(Path::new(primary), agent, path.map(Path::new))
            }
            Self::Remote(remote) => remote.call(Operation::ChildTranscript {
                primary: primary.into(),
                agent: agent.into(),
                path: path.map(str::to_owned),
            }),
        }
    }
    pub fn prepare_input(
        &self,
        session: &str,
        text: &str,
        attachments: &[crate::domain::Attachment],
    ) -> Result<(String, Vec<serde_json::Value>)> {
        match self {
            Self::Local => {
                crate::infrastructure::filesystem::attachments::prompt(text, attachments)
            }
            Self::Remote(remote) => {
                use base64::Engine as _;
                use sha2::{Digest, Sha256};
                let transfer = if attachments
                    .iter()
                    .any(|attachment| attachment.mime.is_none())
                {
                    Some(crate::remote::RemoteHost::connect_profile(
                        &remote.host,
                        &remote.helper,
                        remote.profile.as_deref(),
                    )?)
                } else {
                    None
                };
                crate::infrastructure::filesystem::attachments::prompt_with_paths(
                    text,
                    attachments,
                    |attachment, bytes| {
                        let path: HostPath =
                            transfer.as_ref().unwrap().call(Operation::Upload {
                                session: session.into(),
                                id: attachment.id.clone(),
                                name: attachment.name.clone(),
                                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                                sha256: format!("{:x}", Sha256::digest(bytes)),
                            })?;
                        Ok(path.text())
                    },
                )
            }
        }
    }
}

impl DirectoryBrowser for ExecutionHost {
    fn browse(&self, path: Option<&HostPath>) -> Result<Directory> {
        match self {
            Self::Local => LocalHost.browse(path),
            Self::Remote(host) => host.browse(path),
        }
    }
    fn entered_path(&self, text: &str) -> Result<HostPath> {
        match self {
            Self::Local => LocalHost.entered_path(text),
            Self::Remote(host) => host.entered_path(text),
        }
    }
}

impl ProjectFiles for ExecutionHost {
    fn lists(&self, root: &HostPath) -> Result<(Vec<PathBuf>, Vec<ChangedFile>)> {
        match self {
            Self::Local => LocalHost.lists(root),
            Self::Remote(host) => host.lists(root),
        }
    }
    fn document(
        &self,
        root: &HostPath,
        path: &Path,
        changes: bool,
    ) -> Result<(Snapshot, Option<HostPath>)> {
        match self {
            Self::Local => LocalHost.document(root, path, changes),
            Self::Remote(host) => host.document(root, path, changes),
        }
    }
    fn save(
        &self,
        root: &HostPath,
        path: &Path,
        target: &HostPath,
        expected: &str,
        text: &str,
    ) -> Result<()> {
        match self {
            Self::Local => LocalHost.save(root, path, target, expected, text),
            Self::Remote(host) => host.save(root, path, target, expected, text),
        }
    }
}

#[derive(Clone)]
pub struct NativeStore {
    pub root: HostPath,
    pub host: ExecutionHost,
    pub omp: String,
    local: Option<SessionStore>,
    sync_remote: Option<Arc<RemoteHost>>,
}

impl NativeStore {
    pub fn discover(host: ExecutionHost, omp: &str, cwd: &HostPath) -> Result<Self> {
        let (root, local) = match &host {
            ExecutionHost::Local => {
                let store = SessionStore::discover(omp, cwd.local()?)?;
                (HostPath::Local(store.root.clone()), Some(store))
            }
            ExecutionHost::Remote(remote) => {
                let path: String = remote.call(Operation::Command {
                    omp: omp.into(),
                    cwd: cwd.clone(),
                    args: vec!["config".into(), "path".into()],
                })?;
                (HostPath::posix(path.trim())?, None)
            }
        };
        Ok(Self {
            sync_remote: match &host {
                ExecutionHost::Remote(remote) => Some(Arc::new(RemoteHost::connect_profile(
                    &remote.host,
                    &remote.helper,
                    remote.profile.as_deref(),
                )?)),
                _ => None,
            },
            root,
            host,
            omp: omp.into(),
            local,
        })
    }

    /// Sidebar synchronization never downloads unrelated conversations' bodies.
    /// Its connection is separate from creation, deletion and configuration.
    pub fn sync(
        &self,
        projects: &[HostPath],
        known: &[Registration],
        active: Option<&str>,
        legacy: &Path,
    ) -> Result<Scan> {
        if let Some(store) = &self.local {
            let projects = projects
                .iter()
                .map(|path| path.local().map(Path::to_owned))
                .collect::<Result<Vec<_>>>()?;
            return store.scan(&projects, known, legacy);
        }
        let remote = self.sync_remote.as_ref().context("远端同步连接未初始化")?;
        #[derive(Deserialize)]
        struct Indexed {
            root: HostPath,
            scan: Scan,
        }
        let mut result: Indexed = remote.call(Operation::ScanSessionIndex {
            omp: self.omp.clone(),
            projects: projects.to_vec(),
            known: known.to_vec(),
        })?;
        anyhow::ensure!(
            result.root == self.root,
            "远端 OMP profile 已改变，请重新连接"
        );
        if let Some(binding) =
            active.and_then(|active| known.iter().find(|binding| binding.id == active))
        {
            for native in &mut result.scan.sessions {
                let selected = binding.native_id.as_ref() == Some(&native.id)
                    || binding.omp_session.as_deref()
                        == Some(native.path.to_string_lossy().as_ref());
                if selected && binding.native_revision.as_ref() != Some(&native.revision) {
                    match remote.call::<NativeSession>(Operation::ReadSessionHistory {
                        omp: self.omp.clone(),
                        cwd: HostPath::posix(native.cwd.to_string_lossy())?,
                        path: native.path.to_string_lossy().into_owned(),
                    }) {
                        Ok(full) => *native = full,
                        Err(error) => result
                            .scan
                            .errors
                            .push(format!("读取会话历史失败：{error:#}")),
                    }
                }
            }
        }
        Ok(result.scan)
    }

    pub fn scan(&self, projects: &[HostPath], known: &[Session], legacy: &Path) -> Result<Scan> {
        if let Some(store) = &self.local {
            let projects = projects
                .iter()
                .map(|p| p.local().map(Path::to_owned))
                .collect::<Result<Vec<_>>>()?;
            return store.scan(
                &projects,
                &known.iter().map(Registration::from).collect::<Vec<_>>(),
                legacy,
            );
        }
        let ExecutionHost::Remote(remote) = &self.host else {
            unreachable!()
        };
        #[derive(Deserialize)]
        struct Scanned {
            root: HostPath,
            scan: Scan,
        }
        let result: Scanned = remote.call(Operation::ScanSessions {
            omp: self.omp.clone(),
            projects: projects.to_vec(),
            known: known.iter().map(Registration::from).collect(),
        })?;
        anyhow::ensure!(
            result.root == self.root,
            "远端 OMP profile 已改变，请重新连接"
        );
        Ok(result.scan)
    }

    pub fn create(&self, project: &HostPath, title: &str) -> Result<NativeSession> {
        if let Some(store) = &self.local {
            return store.create(project.local()?, title);
        }
        let ExecutionHost::Remote(remote) = &self.host else {
            unreachable!()
        };
        #[derive(Deserialize)]
        struct Created {
            root: HostPath,
            session: NativeSession,
        }
        let result: Created = remote.call(Operation::CreateSession {
            omp: self.omp.clone(),
            project: project.clone(),
            title: title.into(),
        })?;
        anyhow::ensure!(
            result.root == self.root,
            "远端 OMP profile 已改变，请重新连接"
        );
        Ok(result.session)
    }

    pub fn available(&self, cwd: &HostPath, id: &str) -> Result<()> {
        if let Some(store) = &self.local {
            return store.available(id);
        }
        let ExecutionHost::Remote(remote) = &self.host else {
            unreachable!()
        };
        remote.call(Operation::SessionAvailable {
            omp: self.omp.clone(),
            cwd: cwd.clone(),
            id: id.into(),
        })
    }

    pub fn delete(&self, session: &Session, legacy: &Path) -> Result<()> {
        if let Some(store) = &self.local {
            return store.delete(session, legacy);
        }
        let ExecutionHost::Remote(remote) = &self.host else {
            unreachable!()
        };
        let _: bool = remote.call(Operation::DeleteSession {
            omp: self.omp.clone(),
            session: Box::new(binding(session)),
            empty_only: false,
        })?;
        Ok(())
    }

    pub fn discard_if_untouched(&self, session: &Session, legacy: &Path) -> Result<bool> {
        if !session.is_untouched() {
            return Ok(false);
        }
        if let Some(store) = &self.local {
            return store.discard_if_untouched(session, legacy);
        }
        let ExecutionHost::Remote(remote) = &self.host else {
            unreachable!()
        };
        remote.call(Operation::DeleteSession {
            omp: self.omp.clone(),
            session: Box::new(binding(session)),
            empty_only: true,
        })
    }

    pub fn connect(
        &self,
        root: &HostPath,
        resume: &str,
        id: &str,
    ) -> Result<Box<dyn omp::SessionConnection>> {
        match &self.host {
            ExecutionHost::Local => {
                self.available(root, id)?;
                anyhow::ensure!(Path::new(resume).is_file(), "原生会话已移除，正在同步列表");
                Ok(Box::new(omp::Client::start_project(
                    &self.omp,
                    root.local()?,
                    resume,
                )?))
            }
            ExecutionHost::Remote(remote) => {
                Ok(Box::new(remote.start_session(&self.omp, root, resume)?))
            }
        }
    }
}

fn binding(session: &Session) -> Session {
    let mut binding = Session::new(session.project.clone());
    binding.id = session.id.clone();
    binding.title = session.title.clone();
    binding.location = session.location.clone();
    binding.worktree = session.worktree.clone();
    binding.omp_session = session.omp_session.clone();
    binding.native_id = session.native_id.clone();
    binding.native_root = session.native_root.clone();
    binding
}

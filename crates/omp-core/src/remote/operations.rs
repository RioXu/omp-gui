use super::protocol::*;
use crate::{
    environment::HostPath,
    host::{DirectoryBrowser, LocalHost, ProjectFiles},
    infrastructure::{
        configuration::settings,
        omp::{installation, sessions::SessionStore},
        persistence::workspace_store,
    },
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Resolve POSIX paths only on the Linux host where they exist.
fn native(path: &HostPath) -> Result<&Path> {
    ensure!(cfg!(target_os = "linux"), "远端服务要求 Linux");
    let HostPath::Posix(path) = path else {
        anyhow::bail!("远端请求不能携带客户端本地路径");
    };
    ensure!(
        path.starts_with('/') && !path.contains('\0'),
        "无效远端路径"
    );
    Ok(Path::new(path))
}

fn local(path: &HostPath) -> Result<HostPath> {
    Ok(HostPath::Local(native(path)?.to_owned()))
}
fn remote(path: PathBuf) -> HostPath {
    HostPath::Posix(path.to_string_lossy().into_owned())
}

pub fn execute(operation: Operation) -> Result<Value> {
    match operation {
        Operation::ScanSessionIndex {
            omp,
            projects,
            known,
        } => {
            let projects = projects
                .iter()
                .map(|path| native(path).map(Path::to_owned))
                .collect::<Result<Vec<_>>>()?;
            let cwd = projects
                .first()
                .cloned()
                .unwrap_or(std::env::current_dir()?);
            let store = SessionStore::discover(&omp, &cwd)?;
            let scan = store.scan_index(&projects, &known)?;
            Ok(json!({"root":remote(store.root),"scan":scan}))
        }
        Operation::ReadSessionHistory { omp, cwd, path } => {
            let cwd = native(&cwd)?;
            Ok(serde_json::to_value(
                SessionStore::discover(&omp, cwd)?.read_history(Path::new(&path), cwd)?,
            )?)
        }
        Operation::QueryOperation { id } => {
            let ledger =
                super::receipts::Ledger::new(&workspace_store::data_dir(), &receipt_scope())?;
            Ok(serde_json::to_value(ledger.query(&id)?)?)
        }
        Operation::ChildTranscript {
            primary,
            agent,
            path,
        } => Ok(serde_json::to_value(SessionStore::child_transcript(
            Path::new(&primary),
            &agent,
            path.as_deref().map(Path::new),
        )?)?),
        Operation::ResolveReferences { root, candidates } => {
            ensure!(candidates.len() <= 512, "文件引用数量超过限制");
            let root = native(&root)?;
            let mut resolved = std::collections::BTreeMap::new();
            for candidate in candidates {
                if candidate.len() > 32768 {
                    continue;
                }
                if let Some(path) =
                    crate::infrastructure::filesystem::file_references::resolve(root, &candidate)
                {
                    resolved.insert(
                        candidate,
                        crate::infrastructure::filesystem::file_references::document_path(
                            root, &path,
                        ),
                    );
                }
            }
            Ok(serde_json::to_value(resolved)?)
        }
        Operation::Upload {
            session,
            id,
            name,
            data,
            sha256,
        } => {
            use base64::Engine as _;
            use sha2::{Digest, Sha256};
            ensure!(
                !name.is_empty()
                    && !name.contains(['/', '\\', '\0', '\n', '\r', '"'])
                    && name != "."
                    && name != "..",
                "附件文件名无效"
            );
            ensure!(
                session.len() <= 128
                    && id.len() <= 128
                    && !id.is_empty()
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
                "附件身份无效"
            );
            ensure!(
                data.len()
                    <= crate::infrastructure::filesystem::attachments::MAX_ATTACHMENT.div_ceil(3)
                        * 4,
                "附件超过限制"
            );
            let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
            ensure!(
                bytes.len() <= crate::infrastructure::filesystem::attachments::MAX_ATTACHMENT,
                "附件超过限制"
            );
            ensure!(
                format!("{:x}", Sha256::digest(&bytes)) == sha256,
                "附件完整性校验失败"
            );
            let root = workspace_store::data_dir().join("attachments");
            super::receipts::private_directory(&root)?;
            let root = std::fs::canonicalize(root)?;
            let directory = root.join(crate::domain::hash(&session)).join(&id);
            super::receipts::private_directory(&directory)?;
            let directory = std::fs::canonicalize(directory)?;
            ensure!(directory.starts_with(&root), "附件缓存目录指向异常");
            let path = directory.join(name);
            if path.try_exists()? {
                ensure!(
                    !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
                    "附件缓存不能是符号链接"
                );
                ensure!(std::fs::read(&path)? == bytes, "附件缓存已被修改，未覆盖");
            } else {
                let mut options = std::fs::OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&path)?;
                use std::io::Write as _;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            Ok(serde_json::to_value(remote(std::fs::canonicalize(path)?))?)
        }
        Operation::Configure { action } => crate::host::configuration::execute(*action),
        Operation::Hello { omp } => {
            let executable = installation::resolve(&omp)?;
            let version = settings::command(
                &executable.to_string_lossy(),
                &std::env::current_dir()?,
                &["--version"],
            )?;
            Ok(
                json!({"version":env!("CARGO_PKG_VERSION"), "protocol":VERSION, "capabilities":["directories","files","git","watch","native_sessions","configuration","attachments","supervised_rpc","event_resume","operation_receipts"],"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"home":std::env::var("HOME").unwrap_or_default(),"omp":executable,"omp_version":version.trim()}),
            )
        }
        Operation::Browse { path } => {
            let local_path = path.as_ref().map(local).transpose()?;
            let mut directory = LocalHost.browse(local_path.as_ref())?;
            directory.path = remote(directory.path.local()?.to_owned());
            directory.parent = directory
                .parent
                .map(|p| p.local().map(|p| remote(p.to_owned())))
                .transpose()?;
            for entry in &mut directory.entries {
                entry.path = remote(entry.path.local()?.to_owned());
            }
            Ok(serde_json::to_value(directory)?)
        }
        Operation::Lists { root } => Ok(serde_json::to_value(LocalHost.lists(&local(&root)?)?)?),
        Operation::Document {
            root,
            path,
            changes,
        } => {
            let (snapshot, target) =
                LocalHost.document(&local(&root)?, Path::new(&path), changes)?;
            let target = target
                .map(|p| p.local().map(|p| remote(p.to_owned())))
                .transpose()?;
            Ok(serde_json::to_value((snapshot, target))?)
        }
        Operation::SaveDocument {
            root,
            path,
            target,
            expected,
            text,
        } => {
            LocalHost.save(
                &local(&root)?,
                Path::new(&path),
                &local(&target)?,
                &expected,
                &text,
            )?;
            Ok(Value::Null)
        }
        Operation::ScanSessions {
            omp,
            projects,
            known,
        } => {
            let projects = projects
                .iter()
                .map(|p| native(p).map(Path::to_owned))
                .collect::<Result<Vec<_>>>()?;
            let cwd = projects
                .first()
                .cloned()
                .unwrap_or(std::env::current_dir()?);
            let store = SessionStore::discover(&omp, &cwd)?;
            let scan = store.scan(
                &projects,
                &known,
                &workspace_store::data_dir().join("omp-sessions"),
            )?;
            Ok(json!({"root":remote(store.root),"scan":scan}))
        }
        Operation::CreateSession {
            omp,
            project,
            title,
        } => {
            let project = native(&project)?;
            let store = SessionStore::discover(&omp, project)?;
            Ok(json!({"root":remote(store.root.clone()),"session":store.create(project,&title)?}))
        }
        Operation::DeleteSession {
            omp,
            session,
            empty_only,
        } => {
            let store = SessionStore::discover(&omp, session.cwd())?;
            let legacy = workspace_store::data_dir().join("omp-sessions");
            let removed = if empty_only {
                store.discard_if_untouched(&session, &legacy)?
            } else {
                store.delete(&session, &legacy)?;
                true
            };
            if removed {
                crate::infrastructure::filesystem::attachments::remove_session(
                    &workspace_store::data_dir().join("attachments"),
                    &session.id,
                )?;
            }
            Ok(json!(removed))
        }
        Operation::SessionAvailable { omp, cwd, id } => {
            SessionStore::discover(&omp, native(&cwd)?)?.available(&id)?;
            Ok(Value::Null)
        }
        Operation::Catalog { omp, cwd } => Ok(serde_json::to_value(settings::catalog(
            &omp,
            native(&cwd)?,
        )?)?),
        Operation::Command { omp, cwd, args } => Ok(json!(settings::command(
            &omp,
            native(&cwd)?,
            &args.iter().map(String::as_str).collect::<Vec<_>>()
        )?)),
        Operation::ReadConfig { path } => Ok(json!(settings::read_document(native(&path)?)?)),
        Operation::SaveConfig {
            path,
            expected,
            text,
        } => {
            settings::save_document(native(&path)?, &expected, &text)?;
            Ok(Value::Null)
        }
    }
}

fn receipt_scope() -> String {
    format!(
        "host-{}",
        crate::domain::hash(&format!(
            "{}|{}",
            std::env::var("PI_CODING_AGENT_DIR").unwrap_or_default(),
            std::env::var("PI_CONFIG_DIR").unwrap_or_default()
        ))
    )
}

/// Persist admission before executing an effect, then persist its result before acknowledging.
pub fn dispatch(request: Request) -> Result<Value> {
    ensure!(
        request.protocol == VERSION,
        "远端服务协议不兼容，请更新服务"
    );
    if !request.operation.mutates() {
        return execute(request.operation);
    }
    use super::receipts::{Admission, Ledger};
    let ledger = Ledger::new(&workspace_store::data_dir(), &receipt_scope())?;
    let digest = crate::domain::hash(&serde_json::to_string(&request.operation)?);
    match ledger.begin(&request.id, digest)? {
        Admission::New(commit) => {
            let result = execute(request.operation).map_err(|error| format!("{error:#}"));
            commit.complete(result.clone())?;
            result.map_err(anyhow::Error::msg)
        }
        Admission::Existing(Some(receipt)) if receipt.result.is_some() => {
            receipt.result.unwrap().map_err(anyhow::Error::msg)
        }
        _ => anyhow::bail!("操作已受理，结果待确认；未重复执行"),
    }
}

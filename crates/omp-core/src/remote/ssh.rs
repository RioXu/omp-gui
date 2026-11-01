use super::protocol::*;
use crate::{
    domain::Snapshot,
    environment::HostPath,
    host::{Directory, DirectoryBrowser, ProjectFiles},
    infrastructure::{filesystem::changes::ChangedFile, process::hidden_command},
};
use anyhow::{Context, Result, ensure};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

use super::requests::Work;

#[derive(Clone)]
pub struct RemoteHost {
    tx: mpsc::SyncSender<Work>,
    pub host: String,
    pub helper: String,
    pub profile: Option<String>,
}

pub fn quote(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "远端参数含 NUL");
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

pub fn ssh_command(host: &str, helper: &str, args: &[&str]) -> Result<std::process::Command> {
    let executable = if let Some(relative) = helper.strip_prefix("~/") {
        format!("\"$HOME\"/{}", quote(relative)?)
    } else {
        ensure!(
            helper.starts_with('/'),
            "远端服务路径必须是绝对路径或 ~/ 路径"
        );
        quote(helper)?
    };
    let arguments = args
        .iter()
        .map(|argument| quote(argument))
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    ssh_script(host, &format!("exec {executable} {arguments}"))
}

/// Run an application-owned bootstrap script in the user's remote shell environment.
pub fn ssh_script(host: &str, script: &str) -> Result<std::process::Command> {
    ensure!(
        !host.is_empty()
            && !host.starts_with('-')
            && host.len() <= 255
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"@._-:[]".contains(&b)),
        "SSH Host 无效，请使用 SSH 配置中的主机别名或 user@host"
    );
    let mut command = hidden_command("ssh");
    command
        .args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=3",
            "--",
            host,
        ])
        .arg(format!(
            "exec 3>&1; exec \"${{SHELL:-/bin/sh}}\" -lic {} 1>&2",
            quote(&format!("exec 1>&3 3>&-; {script}"))?
        ));
    Ok(command)
}

impl RemoteHost {
    pub fn connect(host: &str, helper: &str) -> Result<Self> {
        Self::connect_profile(host, helper, None)
    }
    pub fn connect_profile(host: &str, helper: &str, profile: Option<&str>) -> Result<Self> {
        let tx = super::requests::start(host, helper, profile)?;
        Ok(Self {
            tx,
            host: host.into(),
            helper: helper.into(),
            profile: profile.map(str::to_owned),
        })
    }

    pub fn start_session(
        &self,
        omp: &str,
        root: &HostPath,
        resume: &str,
    ) -> Result<super::session::RemoteConnection> {
        super::session::RemoteConnection::start(self.clone(), omp, root, resume)
    }

    pub(super) fn start_bridge(
        &self,
        omp: &str,
        root: &HostPath,
        resume: &str,
        owner: &str,
        after: u64,
        epoch: &str,
    ) -> Result<crate::infrastructure::omp::Client> {
        let HostPath::Posix(root) = root else {
            anyhow::bail!("远端会话要求 POSIX 路径");
        };
        let after = after.to_string();
        let mut arguments = vec![
            "--rpc", "--omp", omp, "--cwd", root, "--resume", resume, "--owner", owner, "--after",
            &after, "--epoch", epoch,
        ];
        if let Some(profile) = self.profile.as_deref() {
            arguments.extend(["--profile", profile]);
        }
        let command = ssh_command(&self.host, &self.helper, &arguments)?;
        crate::infrastructure::omp::Client::from_command(command)
    }

    pub fn call<T: DeserializeOwned>(&self, operation: Operation) -> Result<T> {
        let id = crate::domain::id();
        self.call_with_id(&id, operation)
    }

    /// An explicit operation identity permits querying an acknowledgement after transport loss.
    pub fn call_with_id<T: DeserializeOwned>(&self, id: &str, operation: Operation) -> Result<T> {
        ensure!(!id.is_empty() && id.len() <= 512, "无效操作编号");
        let value = self.call_value(operation, id.into(), true)?;
        serde_json::from_value(value).context("远端响应数据格式不兼容")
    }

    fn call_value(&self, operation: Operation, id: String, recover: bool) -> Result<Value> {
        let mutates = operation.mutates();
        let retry = if recover && !mutates {
            Some(serde_json::to_value(&operation)?)
        } else {
            None
        };
        let (tx, rx) = mpsc::channel();
        let sent = self.tx.try_send((
            Request {
                protocol: VERSION,
                id: id.clone(),
                operation,
            },
            tx,
            Instant::now() + Duration::from_secs(45),
        ));
        if matches!(&sent, Err(mpsc::TrySendError::Full(_))) {
            anyhow::bail!("远端请求队列已满，操作未发送");
        }
        let sent = sent.map_err(|_| anyhow::anyhow!("远端连接已关闭，请重新连接"));
        let mut transport_failure = true;
        let result = sent.and_then(|()| {
            rx.recv_timeout(Duration::from_secs(45))
                .context("远端响应超时，操作结果待确认；未自动重试")?
                .map_err(|error| {
                    transport_failure = error.transport;
                    anyhow::Error::msg(error.message)
                })
        });
        match result {
            Ok(value) => Ok(value),
            Err(error) if recover && transport_failure => {
                let connection =
                    Self::connect_profile(&self.host, &self.helper, self.profile.as_deref())?;
                if mutates {
                    let receipt = connection.call_value(
                        Operation::QueryOperation { id },
                        crate::domain::id(),
                        false,
                    )?;
                    if let Some(receipt) =
                        serde_json::from_value::<Option<super::receipts::Receipt>>(receipt)?
                        && let Some(result) = receipt.result
                    {
                        return result.map_err(anyhow::Error::msg);
                    }
                    Err(error.context("操作结果待确认；未自动重放，请核对远端状态"))
                } else {
                    connection.call_value(
                        serde_json::from_value(retry.context("缺少只读请求")?)?,
                        id,
                        false,
                    )
                }
            }
            Err(error) => Err(error),
        }
    }
}

impl DirectoryBrowser for RemoteHost {
    fn browse(&self, path: Option<&HostPath>) -> Result<Directory> {
        self.call(Operation::Browse {
            path: path.cloned(),
        })
    }
    fn entered_path(&self, text: &str) -> Result<HostPath> {
        HostPath::posix(text.trim())
    }
}

impl ProjectFiles for RemoteHost {
    fn lists(&self, root: &HostPath) -> Result<(Vec<PathBuf>, Vec<ChangedFile>)> {
        self.call(Operation::Lists { root: root.clone() })
    }
    fn document(
        &self,
        root: &HostPath,
        path: &Path,
        changes: bool,
    ) -> Result<(Snapshot, Option<HostPath>)> {
        self.call(Operation::Document {
            root: root.clone(),
            path: path.to_string_lossy().into_owned(),
            changes,
        })
    }
    fn save(
        &self,
        root: &HostPath,
        path: &Path,
        target: &HostPath,
        expected: &str,
        text: &str,
    ) -> Result<()> {
        let result = self.call(Operation::SaveDocument {
            root: root.clone(),
            path: path.to_string_lossy().into_owned(),
            target: target.clone(),
            expected: expected.into(),
            text: text.into(),
        });
        if result.is_err() {
            // A durable acknowledgement may have been lost after the replacement.
            // Only the exact intended content and the same resolved file clear dirty state.
            if let Ok((snapshot, Some(current))) = self.document(root, path, false)
                && &current == target
                && snapshot.text == text
            {
                return Ok(());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_arguments_preserve_paths_without_injecting_shell_or_ssh_options() {
        assert_eq!(
            quote("a'b $(touch /tmp/never)").unwrap(),
            "'a'\\''b $(touch /tmp/never)'"
        );
        assert!(ssh_command("-oProxyCommand=bad", "/bin/helper", &[]).is_err());
        assert!(ssh_command("host; bad", "/bin/helper", &[]).is_err());
        let command = ssh_command("user@host", "~/.local/a b/helper", &["--stdio"]).unwrap();
        let args = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let launch = args.last().unwrap();
        assert!(launch.starts_with("exec 3>&1; exec \"${SHELL:-/bin/sh}\" -lic "));
        assert!(launch.contains(".local/a b/helper") && launch.ends_with(" 1>&2"));
    }
}

//! Deploy verified release binaries, with an embedded-source build fallback.
use anyhow::{Context, Result, ensure};
use omp_core::remote::ssh::{quote, ssh_script};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Output, Stdio},
    time::{Duration, Instant},
};

include!(concat!(env!("OUT_DIR"), "/remote_sources.rs"));

fn collect(mut reader: impl Read) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 4096];
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..count]);
        if output.len() > 32768 {
            output.drain(..output.len() - 32768);
        }
    }
    output
}

fn run(host: &str, script: &str, input: Vec<u8>, timeout: Duration) -> Result<Output> {
    let mut child = ssh_script(host, script)?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("无法启动 SSH 安装连接")?;
    let mut stdin = child.stdin.take().context("SSH stdin missing")?;
    let stdout = child.stdout.take().context("SSH stdout missing")?;
    let stderr = child.stderr.take().context("SSH stderr missing")?;
    let write = std::thread::spawn(move || -> std::io::Result<()> {
        stdin.write_all(&input)?;
        stdin.flush()
    });
    let out = std::thread::spawn(move || collect(stdout));
    let err = std::thread::spawn(move || collect(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("远端服务安装超时；可重连检查安装结果");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = write.join();
    Ok(Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

pub(crate) fn ensure(host: &str, progress: impl Fn(&str)) -> Result<String> {
    let relative = format!(
        ".local/share/omp-gui/remote/{}/omp-gui-remote",
        env!("CARGO_PKG_VERSION")
    );
    let helper = format!("~/{relative}");
    let target = format!("\"$HOME\"/{}", quote(&relative)?);
    progress("正在检查远端服务…");
    let check = run(
        host,
        &format!("if [ -x {target} ]; then {target} --version; else exit 44; fi"),
        Vec::new(),
        Duration::from_secs(30),
    )?;
    let expected = format!(
        "omp-gui-remote/{} protocol/{}",
        env!("CARGO_PKG_VERSION"),
        omp_core::remote::protocol::VERSION
    );
    if check.status.success() && String::from_utf8_lossy(&check.stdout).trim() == expected {
        return Ok(helper);
    }
    // A failed SSH login must not be mistaken for a missing helper.
    ensure!(
        check.status.code() != Some(255),
        "SSH 连接失败：{}",
        String::from_utf8_lossy(&check.stderr).trim()
    );
    let platform = run(host, "uname -sm", Vec::new(), Duration::from_secs(30))?;
    ensure!(platform.status.success(), "无法识别远端平台");
    let platform = String::from_utf8(platform.stdout)?.trim().to_owned();
    let (directory, machine) = match platform.as_str() {
        "Linux x86_64" => ("linux-x86_64", 62u16),
        "Linux aarch64" => ("linux-aarch64", 183u16),
        _ => anyhow::bail!("暂不支持此远端平台：{platform}"),
    };
    let binary = std::env::var_os("OMP_GUI_REMOTE_BINARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| {
                    path.parent()
                        .map(|parent| parent.join("remote").join(directory).join("omp-gui-remote"))
                })
                .filter(|path| path.is_file())
        });
    let install = format!(
        "\"$HOME\"/{}",
        quote(&format!(
            ".local/share/omp-gui/remote/{}",
            env!("CARGO_PKG_VERSION")
        ))?
    );
    let script;
    let payload;
    if let Some(binary) = binary {
        progress("正在上传并校验远端服务…");
        payload = std::fs::read(&binary)
            .with_context(|| format!("无法读取 Linux 服务文件：{}", binary.display()))?;
        ensure!(
            payload.len() >= 20
                && payload.len() <= 128 * 1024 * 1024
                && &payload[..4] == b"\x7fELF"
                && u16::from_le_bytes([payload[18], payload[19]]) == machine,
            "服务文件不是匹配远端架构的 Linux 可执行文件"
        );
        let digest = format!("{:x}", Sha256::digest(&payload));
        let stage = format!(".stage-{}", uuid::Uuid::new_v4());
        script = format!(
            "set -eu; umask 077; mkdir -p {install}; stage={install}/{}; cat > \"$stage\"; printf '%s  %s\\n' {digest} \"$stage\" | sha256sum -c - >/dev/null; chmod 700 \"$stage\"; mv -f \"$stage\" {target}; {target} --version",
            quote(&stage)?
        );
    } else {
        progress("正在构建远端服务（首次安装可能需要几分钟）…");
        let mut archive = tar::Builder::new(Vec::new());
        for (path, bytes) in REMOTE_SOURCES {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            archive.append_data(&mut header, path, *bytes)?;
        }
        archive.finish()?;
        payload = archive.into_inner()?;
        let digest = format!("{:x}", Sha256::digest(&payload));
        let instance = uuid::Uuid::new_v4();
        let build = format!(
            "\"$HOME\"/{}",
            quote(&format!(".cache/omp-gui/build/{digest}-{instance}"))?
        );
        script = format!(
            "set -eu; umask 077; command -v cargo >/dev/null || {{ printf '远端未找到 Rust：请使用包含 Linux 服务的完整安装包，或填写已安装服务路径。' >&2; exit 46; }}; mkdir -p {build} {install}; archive={build}/source.tar; cat > \"$archive\"; printf '%s  %s\\n' {digest} \"$archive\" | sha256sum -c - >/dev/null; tar -xf \"$archive\" -C {build}; cd {build}; export CARGO_TARGET_DIR=\"$HOME/.cache/omp-gui/remote-target/{digest}\"; cargo build --locked --release -p omp-gui-remote; install -m 700 \"$CARGO_TARGET_DIR/release/omp-gui-remote\" {install}/.stage-{instance}; mv -f {install}/.stage-{instance} {target}; {target} --version"
        );
    }
    let result = run(host, &script, payload, Duration::from_secs(15 * 60))?;
    ensure!(
        result.status.success(),
        "远端服务安装失败：{}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    ensure!(
        String::from_utf8_lossy(&result.stdout)
            .lines()
            .any(|line| line.trim() == expected),
        "远端服务版本校验失败"
    );
    progress("远端服务已就绪，正在连接 OMP…");
    Ok(helper)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_remote_source_is_complete_and_has_no_local_state_or_absolute_archive_names() {
        for required in [
            "Cargo.toml",
            "Cargo.lock",
            "crates/omp-core/Cargo.toml",
            "crates/omp-remote/Cargo.toml",
            "crates/omp-remote/src/main.rs",
            "resources/omp-services.json",
        ] {
            assert!(
                REMOTE_SOURCES
                    .iter()
                    .any(|(path, bytes)| *path == required && !bytes.is_empty()),
                "missing {required}"
            );
        }
        for (path, _) in REMOTE_SOURCES {
            assert!(
                !path.starts_with('/')
                    && !path.contains(':')
                    && !path.split('/').any(|part| part == "..")
            );
            assert!(
                !path.starts_with("target/")
                    && !path.contains("workspace.json")
                    && !path.ends_with("remote-development-plan.md")
            );
        }
    }
    #[test]
    #[ignore = "installs the helper on an explicitly authorized SSH test host"]
    fn installs_embedded_remote_service() -> Result<()> {
        let host = std::env::var("OMP_GUI_TEST_INSTALL_SSH_HOST")?;
        let helper = ensure(&host, |stage| eprintln!("{stage}"))?;
        let remote = omp_core::remote::RemoteHost::connect(&host, &helper)?;
        let hello: serde_json::Value =
            remote.call(omp_core::remote::protocol::Operation::Hello { omp: "omp".into() })?;
        ensure!(hello["os"] == "linux", "wrong installed target");
        Ok(())
    }
}

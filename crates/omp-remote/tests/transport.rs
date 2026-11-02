//! Explicit Linux process integration; never contacts a model or user projects.
#![cfg(target_os = "linux")]
use anyhow::{Context, Result, ensure};
use omp_core::{
    environment::HostPath,
    infrastructure::omp::{Client, Event, sessions::NativeSession},
    remote::{
        protocol::{MAX_MESSAGE, Operation, Request, Response, VERSION},
        wire,
    },
};
use serde_json::{Value, json};
use std::{
    io::BufReader,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn event(client: &Client, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        match client
            .events
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))?
        {
            Event::Frame(frame) => {
                eprintln!(
                    "frame type={} state={} id={} error={}",
                    frame["type"], frame["state"], frame["id"], frame["error"]
                );
                if predicate(&frame) {
                    return Ok(frame);
                }
            }
            Event::Error(error) => anyhow::bail!("{error}"),
            Event::Closed => anyhow::bail!("transport closed"),
        }
    }
}

#[test]
#[ignore = "requires explicitly configured OMP_GUI_TEST_OMP; isolated real OMP, no model calls"]
fn binary_management_watch_and_supervised_release() -> Result<()> {
    let omp = std::env::var("OMP_GUI_TEST_OMP").context("set the native OMP executable")?;
    let base = std::env::temp_dir().join(format!("omp-rkyv-{}", omp_core::domain::id()));
    let project = base.join("项目 project");
    let profile = base.join("profile");
    std::fs::create_dir_all(&project)?;
    std::fs::create_dir_all(&profile)?;
    let helper = env!("CARGO_BIN_EXE_omp-gui-remote");
    let command = || {
        let mut cmd = Command::new(helper);
        cmd.env("OPENAI_API_KEY", "omp-gui-isolated-test-no-model-calls")
            .env("OPENAI_BASE_URL", "http://127.0.0.1:1/v1")
            .env("PI_CODING_AGENT_DIR", &profile)
            .env("OMP_GUI_DATA_DIR", base.join("gui"))
            .env("PI_TEST_SESSION_OWNERS_DIR", base.join("owners"))
            .current_dir(&project);
        cmd
    };
    let mut management = Process(
        command()
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut input = management.0.stdin.take().unwrap();
    let mut output = BufReader::new(management.0.stdout.take().unwrap());
    let mut call = |operation| -> Result<Value> {
        let request = Request {
            protocol: VERSION,
            id: omp_core::domain::id(),
            operation,
        };
        wire::write(&mut input, &request, MAX_MESSAGE)?;
        let response: Response = wire::read(&mut output, wire::MAX_RESPONSE)?.context("EOF")?;
        ensure!(
            response.id == request.id && response.protocol == VERSION,
            "identity mismatch"
        );
        response.result.map_err(anyhow::Error::msg)
    };
    let root = HostPath::Posix(project.to_string_lossy().into_owned());
    let hello = call(Operation::Hello { omp: omp.clone() })?;
    ensure!(hello["protocol"] == VERSION, "wrong protocol");
    let uploaded: HostPath = serde_json::from_value(call(Operation::Upload {
        session: "isolated".into(),
        id: "raw-bytes".into(),
        name: "附件.bin".into(),
        data: b"hello".to_vec(),
        sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
    })?)?;
    ensure!(
        std::fs::read(uploaded.text())? == b"hello",
        "binary attachment corrupt"
    );
    let created = call(Operation::CreateSession {
        omp: omp.clone(),
        project: root.clone(),
        title: "binary QA".into(),
    })?;
    let native: NativeSession = serde_json::from_value(created["session"].clone())?;
    for _ in 0..3 {
        let index = call(Operation::ScanSessionIndex {
            omp: omp.clone(),
            projects: vec![root.clone()],
            known: vec![],
        })?;
        ensure!(
            index["scan"]["sessions"].as_array().unwrap().len() == 1,
            "index failed"
        );
    }
    let mut daemon = command();
    let _daemon = Process(daemon.arg("--daemon").spawn()?);
    // Bridges will wait for the private socket to become available.
    std::thread::sleep(Duration::from_millis(100));
    let bridge = |owner: &str| -> Result<Client> {
        let mut cmd = command();
        cmd.args([
            "--rpc",
            "--omp",
            &omp,
            "--cwd",
            &project.to_string_lossy(),
            "--resume",
            &native.path.to_string_lossy(),
            "--owner",
            owner,
            "--after",
            "0",
            "--epoch",
            "",
        ]);
        Client::from_binary_command(cmd)
    };
    let first = bridge("owner-one")?;
    event(&first, |f| f["type"] == "ready")?;
    let rival = bridge("owner-two")?;
    event(&rival, |f| {
        f["type"] == "__omp_gui_transport" && f["state"] == "blocked"
    })?;
    drop(rival);
    // A dropped bridge preserves the native actor; the same owner reattaches.
    drop(first);
    let resumed = bridge("owner-one")?;
    event(&resumed, |f| f["type"] == "ready")?;
    resumed.send(json!({"type":"get_state","id":"still-alive"}))?;
    event(&resumed, |f| f["id"] == "still-alive")?;
    resumed.send(json!({"type":"__omp_gui_release_idle","id":"release"}))?;
    let released = event(&resumed, |f| f["type"] == "__omp_gui_released")?;
    ensure!(released["success"] == true, "release failed: {released}");
    call(Operation::SessionAvailable {
        omp: omp.clone(),
        cwd: root,
        id: native.id,
    })?;

    let mut watch = Process(
        command()
            .args(["--watch", "--cwd", &project.to_string_lossy()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut watch_output = watch.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(wire::read::<omp_core::host::FileChange>(
            &mut watch_output,
            MAX_MESSAGE,
        ));
    });
    let path = project.join("通知.txt");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        std::fs::write(&path, "changed")?;
        if let Ok(change) = rx.recv_timeout(Duration::from_millis(300)) {
            ensure!(
                change?
                    .context("watch EOF")?
                    .affects(&HostPath::Local(path.clone())),
                "missing changed path"
            );
            break;
        }
        ensure!(std::time::Instant::now() < deadline, "watch timeout");
    }
    drop(watch);
    drop(resumed);
    drop(_daemon);
    drop(management);
    std::fs::remove_dir_all(&base)?;
    Ok(())
}

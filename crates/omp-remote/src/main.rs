//! No GPUI dependency; this executable runs inside the remote user's SSH environment.
use anyhow::{Context, Result, ensure};
use omp_core::remote::{
    operations,
    protocol::{MAX_MESSAGE, Request, Response, VERSION},
};
use std::io::{BufRead, Read, Write};
#[cfg(unix)]
mod daemon;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|a| a == "--version") {
        println!(
            "omp-gui-remote/{} protocol/{VERSION}",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    ensure!(cfg!(target_os = "linux"), "远端服务要求 Linux");
    if let Some(profile) = args
        .iter()
        .position(|argument| argument == "--profile")
        .and_then(|index| args.get(index + 1))
    {
        ensure!(
            std::path::Path::new(profile).is_absolute(),
            "远端 OMP profile 必须是绝对路径"
        );
        unsafe {
            std::env::set_var("PI_CODING_AGENT_DIR", profile);
        }
    }
    restore_user_path();
    #[cfg(unix)]
    if args.iter().any(|argument| argument == "--daemon") {
        return daemon::serve();
    }
    if args.iter().any(|argument| argument == "--watch") {
        return watch(&args);
    }
    if args.iter().any(|a| a == "--rpc") {
        return rpc(&args);
    }
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_MESSAGE + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        ensure!(
            count <= MAX_MESSAGE && line.ends_with(b"\n"),
            "远端请求超过限制或未完整传输"
        );
        let request: Request = serde_json::from_slice(&line).context("无效远端请求")?;
        let id = request.id.clone();
        let result = operations::dispatch(request).map_err(|error| format!("{error:#}"));
        let response = Response {
            protocol: VERSION,
            id,
            result,
        };
        omp_core::remote::response::write(&mut output, &response)?;
    }
    Ok(())
}

fn watch(args: &[String]) -> Result<()> {
    use omp_core::{environment::HostPath, host::ExecutionHost};
    let root = args
        .iter()
        .position(|argument| argument == "--cwd")
        .and_then(|index| args.get(index + 1))
        .context("缺少监听目录")?;
    let external = args
        .windows(2)
        .filter(|pair| pair[0] == "--external")
        .map(|pair| HostPath::Local(pair[1].clone().into()))
        .collect::<Vec<_>>();
    let watcher = ExecutionHost::Local.watch(&HostPath::Local(root.into()), &external)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        let _ = std::io::stdin().read(&mut byte);
        let _ = tx.send(());
    });
    let mut output = std::io::stdout().lock();
    loop {
        if rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_ok()
        {
            break;
        }
        let events = watcher.poll();
        if events.is_empty() {
            continue;
        }
        let links_changed = events.into_iter().try_fold(false, |links, event| {
            event
                .map(|event| links || event.links_changed)
                .map_err(anyhow::Error::msg)
        })?;
        serde_json::to_writer(&mut output, &omp_core::host::FileChange { links_changed })?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn restore_user_path() {
    if omp_core::infrastructure::omp::installation::resolve("omp").is_ok() {
        return;
    }
    // SSH exec often omits paths established by the user's interactive shell.
    let Some(shell) = std::env::var_os("SHELL") else {
        return;
    };
    let Ok(mut child) = std::process::Command::new(shell)
        .args(["-lic", "printf '__OMP_GUI_PATH__%s\\n' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return;
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if let Ok(output) = child.wait_with_output() {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(path) = line.strip_prefix("__OMP_GUI_PATH__") {
                // Startup is single-threaded; do not mutate the environment after serving begins.
                unsafe {
                    std::env::set_var("PATH", path);
                }
            }
        }
    }
}

#[cfg(unix)]
fn rpc(args: &[String]) -> Result<()> {
    daemon::bridge(args)
}

#[cfg(not(unix))]
fn rpc(_: &[String]) -> Result<()> {
    anyhow::bail!("远端服务要求 Linux")
}

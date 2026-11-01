//! A private per-user/profile supervisor. SSH bridges may detach without killing OMP.
mod session;

use anyhow::{Context, Result, ensure};
use omp_core::{domain::hash, remote::receipts::private_directory};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Attach {
    pub owner: String,
    pub root: String,
    pub resume: String,
    pub omp: String,
    pub after: u64,
    pub epoch: String,
}

struct Handle {
    tx: mpsc::SyncSender<session::Input>,
    alive: Arc<AtomicBool>,
    root: PathBuf,
}
type Registry = Arc<Mutex<HashMap<PathBuf, Handle>>>;

fn socket() -> Result<PathBuf> {
    // A short private pathname also works when HOME exceeds sockaddr_un's limit.
    let uid = unsafe { libc::geteuid() };
    let root = PathBuf::from(format!("/tmp/omp-gui-{uid}"));
    private_directory(&root)?;
    let meta = std::fs::symlink_metadata(&root)?;
    ensure!(
        meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0,
        "远端服务运行目录权限不安全"
    );
    let scope = format!(
        "{}|{}|{}|{}|{}",
        std::env::var("PI_CODING_AGENT_DIR").unwrap_or_default(),
        std::env::var("PI_CONFIG_DIR").unwrap_or_default(),
        std::env::var("OMP_GUI_DATA_DIR").unwrap_or_default(),
        omp_core::remote::protocol::VERSION,
        env!("CARGO_PKG_VERSION")
    );
    Ok(root.join(format!("{}.sock", hash(&scope))))
}

pub fn serve() -> Result<()> {
    let path = socket()?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.with_extension("lock"))?;
    if lock.try_lock().is_err() {
        return Ok(());
    }
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        ensure!(
            meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() },
            "远端服务地址不是当前用户的 socket"
        );
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
    let mut last_active = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                last_active = Instant::now();
                let registry = registry.clone();
                std::thread::spawn(move || {
                    if let Err(error) = attach(stream, registry) {
                        eprintln!("attach: {error:#}");
                    }
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        let mut actors = registry
            .lock()
            .map_err(|_| anyhow::anyhow!("服务状态不可用"))?;
        actors.retain(|_, actor| actor.alive.load(Ordering::Acquire));
        if !actors.is_empty() {
            last_active = Instant::now();
        }
        if last_active.elapsed() > Duration::from_secs(300) {
            break;
        }
        drop(actors);
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::remove_file(path)?;
    Ok(())
}

fn attach(mut stream: UnixStream, registry: Registry) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let line = read_line(&mut reader, 32768)?.context("缺少附着请求")?;
    let mut request: Attach = serde_json::from_slice(&line)?;
    ensure!(
        request.owner.len() <= 128 && !request.owner.is_empty(),
        "无效连接身份"
    );
    let result = (|| -> Result<mpsc::SyncSender<session::Input>> {
        let root = std::fs::canonicalize(&request.root)?;
        let path = std::fs::canonicalize(&request.resume)?;
        request.root = root.to_string_lossy().into_owned();
        request.resume = path.to_string_lossy().into_owned();
        let mut actors = registry
            .lock()
            .map_err(|_| anyhow::anyhow!("服务状态不可用"))?;
        if let Some(actor) = actors
            .get(&path)
            .filter(|actor| actor.alive.load(Ordering::Acquire))
        {
            ensure!(actor.root == root, "会话不属于当前项目");
            return Ok(actor.tx.clone());
        }
        let store =
            omp_core::infrastructure::omp::sessions::SessionStore::discover(&request.omp, &root)?;
        let native = store.inspect(&path, &root)?;
        store.available(&native.id)?;
        let key = hash(&request.resume);
        omp_core::remote::receipts::Ledger::new(
            &omp_core::infrastructure::persistence::workspace_store::data_dir(),
            &format!("native-{key}"),
        )?;
        let (tx, rx) = mpsc::sync_channel(256);
        let alive = Arc::new(AtomicBool::new(true));
        let alive_actor = alive.clone();
        let startup = request.clone();
        std::thread::spawn(move || {
            if let Err(error) = session::run(startup, rx, &key) {
                eprintln!("session: {error:#}");
            }
            alive_actor.store(false, Ordering::Release);
        });
        actors.insert(
            path,
            Handle {
                tx: tx.clone(),
                alive,
                root,
            },
        );
        Ok(tx)
    })();
    let tx = match result {
        Ok(tx) => tx,
        Err(error) => {
            stream.write_all(&omp_core::infrastructure::omp::encode_transport_frame(&serde_json::json!({"type":"__omp_gui_transport","state":"blocked","error":format!("{error:#}")}))?)?;
            return Ok(());
        }
    };
    stream.set_read_timeout(None)?;
    let generation = omp_core::domain::id();
    tx.send(session::Input::Attach {
        request,
        generation: generation.clone(),
        stream,
    })
    .context("会话已退出，请重连")?;
    while let Some(line) = read_line(&mut reader, omp_core::remote::protocol::MAX_MESSAGE)? {
        let frame = serde_json::from_slice(&line)?;
        if tx
            .try_send(session::Input::Frame {
                generation: generation.clone(),
                frame,
            })
            .is_err()
        {
            break;
        }
    }
    let _ = tx.send(session::Input::Detach(generation));
    Ok(())
}

pub(super) fn read_line(reader: &mut impl BufRead, max: usize) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let count = reader.take((max + 1) as u64).read_until(b'\n', &mut line)?;
    if count == 0 {
        return Ok(None);
    }
    ensure!(
        count <= max && line.ends_with(b"\n"),
        "远端传输不完整或超过限制"
    );
    Ok(Some(line))
}

pub fn bridge(args: &[String]) -> Result<()> {
    let option = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
            .with_context(|| format!("缺少参数 {name}"))
    };
    let request = Attach {
        owner: option("--owner")?,
        root: option("--cwd")?,
        resume: option("--resume")?,
        omp: option("--omp")?,
        after: option("--after")?.parse()?,
        epoch: option("--epoch")?,
    };
    let path = socket()?;
    let mut stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(_) => {
            let mut command = Command::new(std::env::current_exe()?);
            command
                .arg("--daemon")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(profile) = args.windows(2).find(|pair| pair[0] == "--profile") {
                command.args(profile);
            }
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            let mut child = command.spawn().context("无法启动远端监督服务")?;
            // Reap the bootstrap without tying the resident daemon to this SSH process.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match UnixStream::connect(&path) {
                    Ok(stream) => break stream,
                    Err(error) => {
                        if Instant::now() >= deadline {
                            return Err(error.into());
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }
    };
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    let mut writer = stream.try_clone()?;
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut buffer = [0u8; 8192];
        while let Ok(count) = input.read(&mut buffer) {
            if count == 0 || writer.write_all(&buffer[..count]).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Both);
    });
    let mut output = std::io::stdout().lock();
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        output.flush()?;
    }
    Ok(())
}

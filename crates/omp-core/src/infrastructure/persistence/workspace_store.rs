use crate::domain::Database;
use anyhow::Context;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
};

pub fn data_dir() -> PathBuf {
    std::env::var_os("OMP_GUI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(not(windows))]
            {
                std::env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME")
                            .map(|home| PathBuf::from(home).join(".local/share"))
                    })
                    .unwrap_or_else(std::env::temp_dir)
                    .join("omp-gui")
            }
            #[cfg(windows)]
            {
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("OMP GUI")
            }
        })
}
pub fn load() -> anyhow::Result<Database> {
    let p = data_dir().join("workspace.json");
    if !p.exists() {
        return Ok(Database {
            omp_path: crate::infrastructure::omp::installation::executable(),
            ..Database::default()
        });
    }
    let mut db: Database =
        serde_json::from_slice(&fs::read(&p)?).context("无法读取会话数据，原文件未改动")?;
    anyhow::ensure!(
        matches!(db.version, 1 | Database::VERSION),
        "会话文件版本不兼容，原文件未改动"
    );
    migrate_environments(&mut db);
    Ok(db)
}

fn migrate_environments(db: &mut Database) {
    db.version = Database::VERSION;
    use crate::environment::{Connection, Environment, HostPath, ProjectLocation};
    if !db
        .environments
        .iter()
        .any(|environment| environment.id == "local")
    {
        db.environments.insert(0, Environment::default());
    }
    if let Some(local) = db
        .environments
        .iter_mut()
        .find(|environment| environment.id == "local")
    {
        local.connection = Connection::Local;
        local.omp_override =
            (db.omp_path != "omp" && !db.omp_path.trim().is_empty()).then(|| db.omp_path.clone());
    }
    if !db
        .environments
        .iter()
        .any(|environment| environment.id == db.active_environment)
    {
        db.active_environment = "local".into();
    }
    for session in &mut db.sessions {
        session.location.get_or_insert_with(|| ProjectLocation {
            environment: "local".into(),
            profile: None,
            path: HostPath::Local(session.project.clone()),
        });
    }
}
pub fn lock_data() -> anyhow::Result<fs::File> {
    fs::create_dir_all(data_dir())?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_dir().join("workspace.lock"))?;
    file.try_lock()
        .context("此数据目录已在另一个 OMP GUI 窗口中打开；当前窗口只读")?;
    Ok(file)
}
#[derive(Default)]
struct PendingSave {
    snapshot: Option<Database>,
    closing: bool,
}

pub struct Persistence {
    pending: Arc<(Mutex<PendingSave>, Condvar)>,
    done: mpsc::Receiver<()>,
    pub errors: mpsc::Receiver<String>,
}
impl Default for Persistence {
    fn default() -> Self {
        Self::new()
    }
}
impl Persistence {
    #[cfg(any(test, feature = "test-support"))]
    pub fn disconnected() -> Self {
        let (_, errors) = mpsc::channel();
        let (_, done) = mpsc::channel();
        Self {
            pending: Default::default(),
            done,
            errors,
        }
    }
    pub fn new() -> Self {
        Self::with_writer(save_bytes)
    }
    fn with_writer(
        mut write: impl FnMut(&[u8], &mut bool) -> anyhow::Result<()> + Send + 'static,
    ) -> Self {
        let pending: Arc<(Mutex<PendingSave>, Condvar)> = Default::default();
        let worker = pending.clone();
        let (err_tx, errors) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        thread::spawn(move || {
            let mut checked = false;
            loop {
                let (lock, wake) = &*worker;
                let mut state = lock.lock().unwrap();
                while state.snapshot.is_none() && !state.closing {
                    state = wake.wait(state).unwrap();
                }
                let Some(snapshot) = state.snapshot.take() else {
                    break;
                };
                drop(state);
                let result =
                    workspace_bytes(&snapshot).and_then(|bytes| write(&bytes, &mut checked));
                if let Err(error) = result {
                    let _ = err_tx.send(format!("保存失败：{error:#}"));
                }
            }
            let _ = done_tx.send(());
        });
        Self {
            pending,
            done,
            errors,
        }
    }
    pub fn save(&self, db: &Database) -> anyhow::Result<()> {
        // Session bodies are shared. Replacing the one pending snapshot bounds
        // memory even if disk writes are slower than streaming updates.
        anyhow::ensure!(
            matches!(self.done.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "保存服务已停止"
        );
        let snapshot = db.clone();
        let (lock, wake) = &*self.pending;
        let old = lock
            .lock()
            .map_err(|_| anyhow::anyhow!("保存服务已停止"))?
            .snapshot
            .replace(snapshot);
        drop(old);
        wake.notify_one();
        Ok(())
    }
}
fn workspace_bytes(db: &Database) -> anyhow::Result<Vec<u8>> {
    #[derive(serde::Serialize)]
    struct Saved<'a> {
        version: u32,
        sessions: Vec<&'a crate::domain::Session>,
        projects: &'a [PathBuf],
        active: Option<&'a str>,
        omp_path: &'a str,
        theme: &'a str,
        sidebar_width: f32,
        panel_width: f32,
        environments: &'a [crate::environment::Environment],
        active_environment: &'a str,
        remote_projects: &'a [crate::environment::ProjectLocation],
    }
    let sessions: Vec<_> = db.sessions.iter().filter(|s| !s.is_blank()).collect();
    let active = db
        .active
        .as_deref()
        .filter(|id| sessions.iter().any(|s| s.id == *id));
    Ok(sonic_rs::to_vec(&Saved {
        version: db.version,
        sessions,
        projects: &db.projects,
        active,
        omp_path: &db.omp_path,
        theme: &db.theme,
        sidebar_width: db.sidebar_width,
        panel_width: db.panel_width,
        environments: &db.environments,
        active_environment: &db.active_environment,
        remote_projects: &db.remote_projects,
    })?)
}
impl Drop for Persistence {
    fn drop(&mut self) {
        let (lock, wake) = &*self.pending;
        if let Ok(mut state) = lock.lock() {
            state.closing = true;
        }
        wake.notify_one();
        let _ = self.done.recv_timeout(std::time::Duration::from_secs(5));
    }
}
fn save_bytes(bytes: &[u8], upgrade_checked: &mut bool) -> anyhow::Result<()> {
    let dir = data_dir();
    save_bytes_at(&dir, bytes, upgrade_checked)
}
fn save_bytes_at(
    dir: &std::path::Path,
    bytes: &[u8],
    upgrade_checked: &mut bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join("workspace.json.tmp");
    let final_path = dir.join("workspace.json");
    if !*upgrade_checked && final_path.is_file() {
        #[derive(serde::Deserialize)]
        struct Header {
            version: u32,
        }
        let old = fs::read(&final_path)?;
        if serde_json::from_slice::<Header>(&old).is_ok_and(|header| header.version == 1) {
            let backup = dir.join("workspace.v1.backup.json");
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
            {
                Ok(mut backup) => {
                    backup.write_all(&old)?;
                    backup.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("无法备份旧版工作区，原文件未改动"),
            }
        }
    }
    let mut f = fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, final_path)?;
    *upgrade_checked = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_writer_coalesces_pending_snapshots_and_flushes_latest_on_drop() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let output = writes.clone();
        let writer = Persistence::with_writer(move |bytes, _| {
            let db: Database = serde_json::from_slice(bytes)?;
            let first = output.lock().unwrap().is_empty();
            output.lock().unwrap().push(db.sessions[0].draft.clone());
            if first {
                entered_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }
            Ok(())
        });
        let mut db = Database::default();
        let mut session = crate::domain::Session::new("fixture".into());
        session.draft = "one".into();
        db.sessions.push(session);
        writer.save(&db).unwrap();
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        db.sessions[0].draft = "two".into();
        writer.save(&db).unwrap();
        db.sessions[0].draft = "three".into();
        writer.save(&db).unwrap();
        // Encoding and disk work remain blocked while submissions complete.
        assert_eq!(writes.lock().unwrap().as_slice(), &["one"]);
        resume_tx.send(()).unwrap();
        drop(writer);
        assert_eq!(writes.lock().unwrap().as_slice(), &["one", "three"]);
    }
    #[test]
    fn sonic_workspace_preserves_json_values_and_filters_blank_active_session() {
        let mut db = Database::default();
        let mut session = crate::domain::Session::new("中文 project".into());
        session.draft = "中文😀\"\\\n".into();
        session.todos.push(serde_json::json!({"fraction": 0.1234567890123456, "large": u64::MAX, "small": i64::MIN}));
        db.sessions.push(session);
        db.sessions
            .push(crate::domain::Session::new("blank".into()));
        db.active = Some(db.sessions[1].id.clone());
        let value: serde_json::Value =
            serde_json::from_slice(&workspace_bytes(&db).unwrap()).unwrap();
        assert!(value["active"].is_null());
        assert_eq!(value["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["sessions"][0],
            serde_json::to_value(&db.sessions[0]).unwrap()
        );
    }
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn workspace_performance_baseline() {
        use std::{hint::black_box, time::Instant};
        let mut db = Database::default();
        for ix in 0..200 {
            let mut s = crate::domain::Session::new("fixture".into());
            s.title = format!("session {ix}");
            for _ in 0..50 {
                s.messages.push(crate::domain::Message::new(
                    "assistant",
                    "stream 中文 ".repeat(80),
                ));
            }
            db.sessions.push(s);
        }
        let before = Instant::now();
        for _ in 0..1000 {
            black_box(db.clone());
        }
        eprintln!("shared snapshot average: {:?}", before.elapsed() / 1000);
        let before = Instant::now();
        for _ in 0..10 {
            black_box(serde_json::to_vec(&db).unwrap());
        }
        eprintln!("serde_json encode average: {:?}", before.elapsed() / 10);
        let before = Instant::now();
        for _ in 0..10 {
            black_box(workspace_bytes(&db).unwrap());
        }
        eprintln!(
            "sonic-rs encode average: {:?}, bytes: {}",
            before.elapsed() / 10,
            workspace_bytes(&db).unwrap().len()
        );
        let snapshot = db.clone();
        let before = Instant::now();
        db.sessions[0].draft = "changed".into();
        eprintln!("first session copy-on-write: {:?}", before.elapsed());
        black_box(snapshot);
    }
    #[test]
    fn upgrading_workspace_preserves_an_exact_original_backup() {
        let root =
            std::env::temp_dir().join(format!("omp-workspace-upgrade-{}", crate::domain::id()));
        fs::create_dir_all(&root).unwrap();
        let original = br#"{"version":1,"futureField":"preserve exactly"}"#;
        fs::write(root.join("workspace.json"), original).unwrap();
        let mut checked = false;
        save_bytes_at(&root, br#"{"version":2}"#, &mut checked).unwrap();
        save_bytes_at(&root, br#"{"version":2,"next":true}"#, &mut checked).unwrap();
        assert_eq!(
            fs::read(root.join("workspace.v1.backup.json")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(root.join("workspace.json")).unwrap(),
            br#"{"version":2,"next":true}"#
        );
        assert!(
            root.is_absolute()
                && root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-workspace-upgrade-")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_workspace_migrates_to_local_without_losing_native_bindings_or_drafts() {
        let mut db = Database {
            omp_path: "C:/custom omp/omp.exe".into(),
            ..Database::default()
        };
        let mut session = crate::domain::Session::new("project".into());
        session.draft = "未发送内容".into();
        session.native_id = Some("native-id".into());
        session.omp_session = Some("native.jsonl".into());
        db.active = Some(session.id.clone());
        db.sessions.push(session);
        let mut old = serde_json::to_value(&db).unwrap();
        old["version"] = serde_json::json!(1);
        for key in ["environments", "active_environment", "remote_projects"] {
            old.as_object_mut().unwrap().remove(key);
        }
        old["sessions"][0]
            .as_object_mut()
            .unwrap()
            .remove("location");
        let mut migrated: Database = serde_json::from_value(old).unwrap();
        migrate_environments(&mut migrated);
        assert_eq!(
            migrated.environments[0].omp_override.as_deref(),
            Some("C:/custom omp/omp.exe")
        );
        assert_eq!(
            migrated.sessions[0].location.as_ref().unwrap().environment,
            "local"
        );
        let saved: Database = serde_json::from_slice(&workspace_bytes(&migrated).unwrap()).unwrap();
        assert_eq!(saved.active, db.active);
        assert_eq!(saved.sessions[0].draft, "未发送内容");
        assert_eq!(saved.sessions[0].native_id.as_deref(), Some("native-id"));
        assert_eq!(
            saved.sessions[0].omp_session.as_deref(),
            Some("native.jsonl")
        );
        assert_eq!(saved.environments, migrated.environments);
        assert_eq!(saved.version, Database::VERSION);
    }
    #[test]
    fn json_roundtrip_retains_comment_snapshots() {
        let mut db = Database::default();
        let mut s = crate::domain::Session::new("project".into());
        let snapshot = crate::domain::Snapshot::new("file".into(), "before".into(), "after".into());
        s.snapshots.insert(snapshot.id.clone(), snapshot);
        db.sessions.push(s);
        let text = serde_json::to_string(&db).unwrap();
        let copy: Database = serde_json::from_str(&text).unwrap();
        assert_eq!(
            copy.sessions[0].snapshots.values().next().unwrap().text,
            "after"
        );
    }
}

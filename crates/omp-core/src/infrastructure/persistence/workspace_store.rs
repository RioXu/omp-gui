use crate::domain::Database;
use anyhow::Context;
use std::{fs, io::Write, path::PathBuf, sync::mpsc, thread};

pub fn data_dir() -> PathBuf {
    std::env::var_os("OMP_GUI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("OMP GUI")
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
    anyhow::ensure!(db.version == 1, "会话文件版本不兼容，原文件未改动");
    migrate_environments(&mut db);
    Ok(db)
}

fn migrate_environments(db: &mut Database) {
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
pub struct Persistence {
    tx: mpsc::Sender<WriteRequest>,
    pub errors: mpsc::Receiver<String>,
}

impl Default for Persistence {
    fn default() -> Self {
        Self::new()
    }
}
enum WriteRequest {
    Save(Vec<u8>),
    Flush(mpsc::Sender<()>),
}
impl Persistence {
    #[cfg(any(test, feature = "test-support"))]
    pub fn disconnected() -> Self {
        let (tx, _) = mpsc::channel();
        let (_, errors) = mpsc::channel();
        Self { tx, errors }
    }
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<WriteRequest>();
        let (err_tx, errors) = mpsc::channel();
        thread::spawn(move || {
            for request in rx {
                match request {
                    WriteRequest::Save(bytes) => {
                        if let Err(e) = save_bytes(&bytes) {
                            let _ = err_tx.send(format!("保存失败：{e:#}"));
                        }
                    }
                    WriteRequest::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { tx, errors }
    }
    pub fn save(&self, db: &Database) -> anyhow::Result<()> {
        self.tx
            .send(WriteRequest::Save(workspace_bytes(db)?))
            .context("保存服务已停止")
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
    Ok(serde_json::to_vec(&Saved {
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
        let (tx, rx) = mpsc::channel();
        if self.tx.send(WriteRequest::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
        }
    }
}
fn save_bytes(bytes: &[u8]) -> anyhow::Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    let tmp = dir.join("workspace.json.tmp");
    let final_path = dir.join("workspace.json");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

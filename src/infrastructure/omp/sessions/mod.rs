//! Native JSONL session store shared with terminal OMP, scoped to its profile.
mod history;
mod lease;
mod operations;
mod paths;
pub use paths::plain_path;
use paths::{bucket, validate_path};
#[cfg(test)]
mod tests;
use crate::{
    domain::{Message, Session},
    infrastructure::filesystem::project_paths::project_key,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[derive(Clone, Debug)]
pub struct NativeSession {
    pub id: String,
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub title: String,
    pub updated: u64,
    pub(crate) messages: Arc<Vec<Message>>,
    pub(crate) revision: String,
}
/// Background discovery needs bindings, never GUI snapshots or comment bodies.
pub(crate) struct Registration {
    pub id: String,
    title: String,
    cwd: PathBuf,
    omp_session: Option<String>,
    native_root: Option<String>,
    native_id: Option<String>,
    has_messages: bool,
}
impl From<&Session> for Registration {
    fn from(s: &Session) -> Self {
        Self {
            id: s.id.clone(),
            title: s.title.clone(),
            cwd: s.cwd().to_owned(),
            omp_session: s.omp_session.clone(),
            native_root: s.native_root.clone(),
            native_id: s.native_id.clone(),
            has_messages: !s.messages.is_empty(),
        }
    }
}
type Cache = HashMap<PathBuf, (u64, SystemTime, NativeSession)>;
#[derive(Clone)]
pub struct SessionStore {
    pub root: PathBuf,
    owners: PathBuf,
    cache: Arc<Mutex<Cache>>,
}
pub(crate) struct Scan {
    pub sessions: Vec<NativeSession>,
    pub bindings: Vec<(String, NativeSession)>,
    pub missing: Vec<String>,
    pub errors: Vec<String>,
}
impl SessionStore {
    pub fn discover(omp: &str, cwd: &Path) -> Result<Self> {
        let root = PathBuf::from(
            crate::infrastructure::configuration::settings::command(omp, cwd, &["config", "path"])?
                .trim(),
        );
        ensure!(root.is_absolute(), "OMP 返回的会话根目录无效");
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .context("无法确定用户目录")?;
        let owners = std::env::var_os("PI_TEST_SESSION_OWNERS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let name = std::env::var("PI_CONFIG_DIR")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| ".omp".into());
                PathBuf::from(format!(
                    "{}/{name}/run/session-owners",
                    PathBuf::from(home).display()
                ))
            });
        ensure!(owners.is_absolute(), "OMP 会话占用目录必须是绝对路径");
        Ok(Self {
            root: plain_path(&root),
            owners: plain_path(&owners),
            cache: Default::default(),
        })
    }
    pub fn available(&self, id: &str) -> Result<()> {
        drop(lease::acquire(&self.owners, id)?);
        Ok(())
    }
    pub fn list(&self, projects: &[PathBuf]) -> Result<Vec<NativeSession>> {
        let scan = self.scan(projects, &[], Path::new(""))?;
        ensure!(scan.errors.is_empty(), "{}", scan.errors.join("\n"));
        Ok(scan.sessions)
    }
    pub fn project_dir(&self, cwd: &Path) -> PathBuf {
        let cwd = plain_path(&fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_owned()));
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();
        let home = plain_path(&fs::canonicalize(&home).unwrap_or(home));
        let temp = std::env::temp_dir();
        let temp = plain_path(&fs::canonicalize(&temp).unwrap_or(temp));
        self.root.join("sessions").join(bucket(&cwd, &home, &temp))
    }
    pub(crate) fn scan(
        &self,
        projects: &[PathBuf],
        known: &[Registration],
        legacy: &Path,
    ) -> Result<Scan> {
        let mut result = Scan {
            sessions: vec![],
            bindings: vec![],
            missing: vec![],
            errors: vec![],
        };
        for s in known.iter().filter(|s| s.native_root.is_none()) {
            match self.register(s, legacy) {
                Ok(Some(native)) => result.bindings.push((s.id.clone(), native)),
                Ok(None) => {}
                Err(e) => result.errors.push(format!("{}：{e:#}", s.title)),
            }
        }
        let root = self.root.join("sessions");
        if root.try_exists()? {
            for dir in fs::read_dir(&root)? {
                let dir = dir?;
                if !dir.file_type()?.is_dir() {
                    continue;
                }
                for entry in fs::read_dir(dir.path())? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().is_none_or(|e| e != "jsonl")
                        || !entry.file_type()?.is_file()
                    {
                        continue;
                    }
                    // Read only a small header for unrelated projects.
                    let matches = history::header(&path).ok().is_some_and(|header| {
                        header["cwd"].as_str().is_some_and(|cwd| {
                            projects
                                .iter()
                                .any(|p| project_key(p) == project_key(Path::new(cwd)))
                        })
                    });
                    if !matches {
                        continue;
                    }
                    match self.cached_read(&path) {
                        Ok(native) => result.sessions.push(native),
                        Err(e) => result
                            .errors
                            .push(format!("读取 {} 失败：{e:#}", path.display())),
                    }
                }
            }
        }
        for s in known
            .iter()
            .filter(|s| s.native_root.as_deref() == Some(self.root.to_string_lossy().as_ref()))
        {
            if let Some(path) = &s.omp_session
                && !Path::new(path).try_exists()?
            {
                // OMP may temporarily replace a file; do not treat a recoverable
                // rewrite backup or a relocated native ID as deletion.
                let p = Path::new(path);
                let prefix = format!("{}.", p.file_name().unwrap_or_default().to_string_lossy());
                let backups = p
                    .parent()
                    .filter(|p| p.is_dir())
                    .map(fs::read_dir)
                    .transpose()?
                    .is_some_and(|items| {
                        items.flatten().any(|e| {
                            e.file_name().to_string_lossy().starts_with(&prefix)
                                && e.path().extension().is_some_and(|e| e == "bak")
                        })
                    });
                if !backups
                    && !result
                        .sessions
                        .iter()
                        .any(|n| Some(&n.id) == s.native_id.as_ref())
                {
                    result.missing.push(s.id.clone());
                }
            }
        }
        result.sessions.sort_by_key(|s| s.updated);
        Ok(result)
    }
    fn cached_read(&self, path: &Path) -> Result<NativeSession> {
        let meta = fs::metadata(path)?;
        let stamp = meta.modified()?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("会话缓存不可用"))?;
        if let Some((len, time, native)) = cache.get(path)
            && *len == meta.len()
            && *time == stamp
        {
            return Ok(native.clone());
        }
        let native = history::read(path)?;
        cache.insert(path.to_owned(), (meta.len(), stamp, native.clone()));
        Ok(native)
    }
}

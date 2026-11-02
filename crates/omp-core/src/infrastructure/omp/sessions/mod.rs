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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NativeSession {
    #[serde(default)]
    pub metadata_only: bool,
    pub id: String,
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub title: String,
    pub updated: u64,
    pub messages: Arc<Vec<Message>>,
    pub revision: String,
    pub todos: Vec<Value>,
    pub agents: Vec<Value>,
}
/// Background discovery needs bindings, never GUI snapshots or comment bodies.
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct Registration {
    pub id: String,
    title: String,
    cwd: PathBuf,
    pub(crate) omp_session: Option<String>,
    native_root: Option<String>,
    pub(crate) native_id: Option<String>,
    #[serde(default)]
    pub(crate) native_revision: Option<String>,
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
            native_revision: s.native_revision.clone(),
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
    index: Arc<Mutex<Option<IndexCache>>>,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Scan {
    pub sessions: Vec<NativeSession>,
    pub bindings: Vec<(String, NativeSession)>,
    pub missing: Vec<String>,
    pub errors: Vec<String>,
}
struct IndexCache {
    rows: Vec<NativeSession>,
    dirty: Arc<std::sync::atomic::AtomicBool>,
    changed: Arc<Mutex<std::collections::HashSet<PathBuf>>>,
    _watcher: notify::RecommendedWatcher,
    checked: std::time::Instant,
}

struct IndexRefresh {
    dirty: Arc<std::sync::atomic::AtomicBool>,
    successful: bool,
}
impl Drop for IndexRefresh {
    fn drop(&mut self) {
        if !self.successful {
            self.dirty.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

impl SessionStore {
    pub fn child_transcript(
        primary: &Path,
        agent: &str,
        explicit: Option<&Path>,
    ) -> Result<Vec<Message>> {
        let artifacts = primary.with_extension("");
        let path = explicit
            .map(Path::to_owned)
            .unwrap_or_else(|| artifacts.join(format!("{agent}.jsonl")));
        let base = fs::canonicalize(&artifacts)?;
        let path = fs::canonicalize(path)?;
        ensure!(path.starts_with(base), "子会话不属于当前主会话");
        Self::transcript(&path)
    }
    pub fn transcript(path: &Path) -> Result<Vec<Message>> {
        Ok(history::read(path)?.messages.as_ref().clone())
    }
    pub fn discover(omp: &str, cwd: &Path) -> Result<Self> {
        use std::sync::OnceLock;
        type Stores = Vec<(String, std::time::Instant, SessionStore)>;
        static STORES: OnceLock<Mutex<Stores>> = OnceLock::new();
        let key = format!(
            "{omp}|{}|{:?}|{:?}|{:?}|{:?}",
            cwd.display(),
            std::env::var_os("PI_CODING_AGENT_DIR"),
            std::env::var_os("PI_CONFIG_DIR"),
            std::env::var_os("PI_TEST_SESSION_OWNERS_DIR"),
            std::env::var_os("HOME")
        );
        let stores = STORES.get_or_init(Default::default);
        {
            let stores = stores
                .lock()
                .map_err(|_| anyhow::anyhow!("会话存储缓存不可用"))?;
            if let Some((_, _, store)) = stores
                .iter()
                .find(|(k, at, _)| k == &key && at.elapsed().as_secs() < 30)
            {
                return Ok(store.clone());
            }
        }
        // Process startup must never happen under the shared cache lock.
        let store = Self::discover_uncached(omp, cwd)?;
        let mut stores = stores
            .lock()
            .map_err(|_| anyhow::anyhow!("会话存储缓存不可用"))?;
        let store = stores
            .iter()
            .find(|(_, _, existing)| existing.root == store.root && existing.owners == store.owners)
            .map(|(_, _, existing)| existing.clone())
            .unwrap_or(store);
        stores.retain(|(k, at, _)| k != &key && at.elapsed().as_secs() < 30);
        if stores.len() >= 16 {
            stores.remove(0);
        }
        stores.push((key, std::time::Instant::now(), store.clone()));
        Ok(store)
    }
    fn discover_uncached(omp: &str, cwd: &Path) -> Result<Self> {
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
            index: Default::default(),
        })
    }
    pub fn available(&self, id: &str) -> Result<()> {
        drop(lease::acquire(&self.owners, id)?);
        Ok(())
    }
    /// Validate a requested journal against this profile and the actual project.
    pub fn inspect(&self, path: &Path, cwd: &Path) -> Result<NativeSession> {
        let path = fs::canonicalize(path)?;
        let root = fs::canonicalize(self.root.join("sessions"))?;
        ensure!(path.starts_with(root), "会话不属于当前 OMP profile");
        let native = history::summary(&path)?;
        ensure!(
            fs::canonicalize(&native.cwd)? == fs::canonicalize(cwd)?,
            "会话不属于当前项目"
        );
        Ok(native)
    }
    pub fn read_history(&self, path: &Path, cwd: &Path) -> Result<NativeSession> {
        let summary = self.inspect(path, cwd)?;
        let mut full = self.cached_read(&summary.path)?;
        full.revision = summary.revision;
        Ok(full)
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
    pub fn scan(
        &self,
        projects: &[PathBuf],
        known: &[Registration],
        legacy: &Path,
    ) -> Result<Scan> {
        self.scan_mode(projects, known, legacy, false)
    }
    pub fn scan_index(&self, projects: &[PathBuf], known: &[Registration]) -> Result<Scan> {
        self.scan_mode(projects, known, Path::new(""), true)
    }
    fn scan_mode(
        &self,
        projects: &[PathBuf],
        known: &[Registration],
        legacy: &Path,
        metadata_only: bool,
    ) -> Result<Scan> {
        let mut result = Scan {
            sessions: vec![],
            bindings: vec![],
            missing: vec![],
            errors: vec![],
        };
        for s in known
            .iter()
            .filter(|s| !metadata_only && s.native_root.is_none())
        {
            match self.register(s, legacy) {
                Ok(Some(native)) => result.bindings.push((s.id.clone(), native)),
                Ok(None) => {}
                Err(e) => result.errors.push(format!("{}：{e:#}", s.title)),
            }
        }
        let root = self.root.join("sessions");
        if metadata_only {
            for native in self.index_rows()? {
                if projects
                    .iter()
                    .any(|p| project_key(p) == project_key(&native.cwd))
                {
                    result.sessions.push(native);
                }
            }
        } else if root.try_exists()? {
            for native in self.index_rows()? {
                if !projects
                    .iter()
                    .any(|p| project_key(p) == project_key(&native.cwd))
                {
                    continue;
                }
                match self.cached_read(&native.path) {
                    Ok(native) => result.sessions.push(native),
                    Err(e) => result
                        .errors
                        .push(format!("读取 {} 失败：{e:#}", native.path.display())),
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
    fn index_rows(&self) -> Result<Vec<NativeSession>> {
        use notify::Watcher;
        use std::sync::atomic::{AtomicBool, Ordering};
        let root = self.root.join("sessions");
        if !root.try_exists()? {
            return Ok(Vec::new());
        }
        let mut cached = self
            .index
            .lock()
            .map_err(|_| anyhow::anyhow!("会话索引不可用"))?;
        if let Some(cache) = cached.as_ref()
            && !cache.dirty.load(Ordering::Acquire)
            && cache.changed.lock().is_ok_and(|p| p.is_empty())
            && cache.checked.elapsed().as_secs() < 30
        {
            return Ok(cache.rows.clone());
        }
        if cached.is_none() {
            let dirty = Arc::new(AtomicBool::new(true));
            let signal = dirty.clone();
            let changed = Arc::new(Mutex::new(std::collections::HashSet::new()));
            let changes = changed.clone();
            let watched_root = root.clone();
            let mut watcher = notify::recommended_watcher(
                move |event: notify::Result<notify::Event>| match event {
                    Ok(event) if matches!(event.kind, notify::EventKind::Access(_)) => {}
                    Ok(event) if !event.need_rescan() && !event.paths.is_empty() => {
                        if let Ok(mut dirs) = changes.lock() {
                            for path in event.paths {
                                if let Some(first) = path
                                    .strip_prefix(&watched_root)
                                    .ok()
                                    .and_then(|p| p.components().next())
                                {
                                    dirs.insert(watched_root.join(first));
                                } else {
                                    signal.store(true, Ordering::Release);
                                }
                            }
                            if dirs.len() > 256 {
                                dirs.clear();
                                signal.store(true, Ordering::Release);
                            }
                        } else {
                            signal.store(true, Ordering::Release);
                        }
                    }
                    _ => signal.store(true, Ordering::Release),
                },
            )?;
            watcher.watch(&root, notify::RecursiveMode::Recursive)?;
            *cached = Some(IndexCache {
                rows: Vec::new(),
                dirty,
                changed,
                _watcher: watcher,
                checked: std::time::Instant::now(),
            });
        }
        let cache = cached.as_mut().unwrap();
        let full =
            cache.dirty.swap(false, Ordering::AcqRel) || cache.checked.elapsed().as_secs() >= 30;
        let changed = std::mem::take(
            &mut *cache
                .changed
                .lock()
                .map_err(|_| anyhow::anyhow!("会话变更索引不可用"))?,
        );
        let mut refresh = IndexRefresh {
            dirty: cache.dirty.clone(),
            successful: false,
        };
        let old: HashMap<_, _> = cache.rows.iter().map(|n| (n.path.clone(), n)).collect();
        let directories = if full {
            fs::read_dir(&root)?
                .filter_map(|d| d.ok())
                .filter(|d| d.file_type().is_ok_and(|t| t.is_dir()))
                .map(|d| d.path())
                .collect::<Vec<_>>()
        } else {
            changed.iter().filter(|p| p.is_dir()).cloned().collect()
        };
        let mut rows = if full {
            Vec::new()
        } else {
            cache
                .rows
                .iter()
                .filter(|n| n.path.parent().is_some_and(|p| !changed.contains(p)))
                .cloned()
                .collect()
        };
        for dir in directories {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "jsonl") || !entry.file_type()?.is_file() {
                    continue;
                }
                let meta = entry.metadata()?;
                let stamp = meta
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let revision = format!("index-v1:{}:{}", meta.len(), stamp.as_nanos());
                if let Some(previous) = old.get(&path).filter(|n| n.revision == revision) {
                    rows.push((*previous).clone());
                } else if let Ok(summary) = history::summary(&path) {
                    rows.push(summary);
                }
            }
        }
        cache.rows = rows;
        if full {
            cache.checked = std::time::Instant::now();
        }
        refresh.successful = true;
        Ok(cache.rows.clone())
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
        // Bound retained histories by source size and count. Very large histories
        // are served without retaining another long-lived projection.
        cache.retain(|_, (len, _, _)| *len <= 32 * 1024 * 1024);
        if cache.len() >= 3
            || cache.values().map(|(len, _, _)| *len).sum::<u64>() + meta.len() > 32 * 1024 * 1024
        {
            cache.clear();
        }
        if meta.len() <= 32 * 1024 * 1024 {
            cache.insert(path.to_owned(), (meta.len(), stamp, native.clone()));
        }
        Ok(native)
    }
}

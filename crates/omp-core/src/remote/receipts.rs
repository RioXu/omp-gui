//! Durable admission records prevent uncertain network failures from replaying effects.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const LIMIT: usize = 10000;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub digest: String,
    pub accepted_at: u64,
    pub result: Option<Result<Value, String>>,
}

#[derive(Clone)]
pub struct Ledger {
    root: PathBuf,
    count: Arc<AtomicUsize>,
}
pub enum Admission {
    New(Commit),
    Existing(Option<Receipt>),
}
pub struct Commit {
    path: PathBuf,
    receipt: Receipt,
    _lock: fs::File,
}

pub fn private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() },
            "远端缓存目录不属于当前用户或是符号链接"
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn atomic(path: &Path, receipt: &Receipt) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        serde_json::to_writer(&mut file, receipt)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        #[cfg(unix)]
        fs::File::open(path.parent().context("操作记录没有父目录")?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

impl Ledger {
    pub fn new(base: &Path, namespace: &str) -> Result<Self> {
        ensure!(
            !namespace.is_empty()
                && namespace
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "无效操作记录作用域"
        );
        let root = base.join("receipts").join(namespace);
        private_directory(&root)?;
        let mut count = 0;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                count += 1;
            }
        }
        Ok(Self {
            root,
            count: Arc::new(AtomicUsize::new(count)),
        })
    }
    fn path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{}.json", crate::domain::hash(id)))
    }
    pub fn query(&self, id: &str) -> Result<Option<Receipt>> {
        let path = self.path(id);
        if !path.try_exists()? {
            return Ok(None);
        }
        let file = fs::File::open(&path)?;
        ensure!(
            file.metadata()?.len() <= super::protocol::MAX_MESSAGE as u64,
            "操作结果超过限制"
        );
        Ok(Some(
            serde_json::from_reader(file).context("操作记录损坏；未重放指令")?,
        ))
    }
    pub fn begin(&self, id: &str, digest: String) -> Result<Admission> {
        ensure!(!id.is_empty() && id.len() <= 512, "无效操作编号");
        let path = self.path(id);
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).write(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path.with_extension("lock"))?;
        if lock.try_lock().is_err() {
            let receipt = self.query(id)?;
            ensure!(
                receipt
                    .as_ref()
                    .is_none_or(|receipt| receipt.digest == digest),
                "同一操作编号包含不同内容，未执行"
            );
            return Ok(Admission::Existing(receipt));
        }
        if let Some(receipt) = self.query(id)? {
            ensure!(receipt.digest == digest, "同一操作编号包含不同内容，未执行");
            return Ok(Admission::Existing(Some(receipt)));
        }
        ensure!(
            self.count.fetch_add(1, Ordering::Relaxed) < LIMIT,
            "操作记录已达上限；请保留未确认记录并清理已完成记录"
        );
        let receipt = Receipt {
            digest,
            accepted_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            result: None,
        };
        atomic(&path, &receipt)?;
        Ok(Admission::New(Commit {
            path,
            receipt,
            _lock: lock,
        }))
    }
}

impl Commit {
    pub fn complete(mut self, result: Result<Value, String>) -> Result<()> {
        self.receipt.result = Some(result);
        atomic(&self.path, &self.receipt)
    }
}

pub fn native_mutation(kind: &str) -> bool {
    !kind.starts_with("get_")
        && !matches!(
            kind,
            "negotiate_protocol"
                | "set_event_filter"
                | "set_subagent_subscription"
                | "predict_word"
                | "predict_word_feedback"
                | "__omp_gui_query"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admissions_survive_disconnects_and_process_recreation_without_replaying_unknown_effects() {
        let root = std::env::temp_dir().join(format!("omp-receipts-{}", uuid::Uuid::new_v4()));
        let ledger = Ledger::new(&root, "host").unwrap();
        let Admission::New(commit) = ledger.begin("request", "digest".into()).unwrap() else {
            panic!("first admission");
        };
        assert!(matches!(
            ledger.begin("request", "digest".into()).unwrap(),
            Admission::Existing(Some(Receipt { result: None, .. }))
        ));
        drop(commit);
        let reopened = Ledger::new(&root, "host").unwrap();
        assert!(matches!(
            reopened.begin("request", "digest".into()).unwrap(),
            Admission::Existing(Some(Receipt { result: None, .. }))
        ));
        assert!(reopened.begin("request", "different".into()).is_err());
        let Admission::New(commit) = reopened.begin("complete", "digest".into()).unwrap() else {
            panic!("new request");
        };
        commit
            .complete(Ok(serde_json::json!({"saved":true})))
            .unwrap();
        assert_eq!(
            reopened
                .query("complete")
                .unwrap()
                .unwrap()
                .result
                .unwrap()
                .unwrap()["saved"],
            true
        );
        assert!(
            root.is_absolute()
                && root.starts_with(std::env::temp_dir())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("omp-receipts-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

use super::*;

impl SessionStore {
    pub fn remove(&self, native: &NativeSession) -> Result<()> {
        let _lease = lease::acquire(&self.owners, &native.id)?;
        self.delete_native(&native.path, &native.id)
    }
    fn delete_native(&self, path: &Path, id: &str) -> Result<()> {
        validate_path(path, &self.root.join("sessions"))?;
        if path.try_exists()? {
            ensure!(
                history::header(path)?["id"] == id,
                "原生会话编号已改变，未删除"
            );
        }
        delete_artifacts(path)
    }

    pub fn create(&self, cwd: &Path, title: &str) -> Result<NativeSession> {
        ensure!(cwd.is_dir(), "项目目录不存在");
        let cwd = plain_path(&fs::canonicalize(cwd)?);
        let dir = self.project_dir(&cwd);
        fs::create_dir_all(&dir)?;
        let id = uuid::Uuid::new_v4().to_string();
        let _lease = lease::acquire(&self.owners, &id)?;
        let time = chrono::Utc::now();
        let path = dir.join(format!(
            "{}_{}.jsonl",
            time.format("%Y-%m-%dT%H-%M-%S-%3fZ"),
            id
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        writeln!(
            file,
            "{}",
            json!({"type":"session","version":3,"id":id,"timestamp":time.to_rfc3339_opts(chrono::SecondsFormat::Millis,true),"cwd":cwd,"title":title,"titleSource":"auto"})
        )?;
        file.sync_all()?;
        history::read(&path)
    }
    pub(super) fn register(
        &self,
        s: &Registration,
        legacy: &Path,
    ) -> Result<Option<NativeSession>> {
        let Some(old) = s.omp_session.as_deref().map(Path::new) else {
            if !s.has_messages {
                return self.create(&s.cwd, &s.title).map(Some);
            }
            anyhow::bail!("没有原生记录，已保留 GUI 历史，无法自动迁移");
        };
        if !old.try_exists()? {
            if !s.has_messages {
                return self.create(&s.cwd, &s.title).map(Some);
            }
            anyhow::bail!("原生记录已缺失，已保留 GUI 历史");
        }
        let original = history::read(old)?;
        ensure!(
            project_key(&original.cwd) == project_key(&s.cwd),
            "原生记录的工作目录与 GUI 会话不一致"
        );
        if !plain_path(old).starts_with(plain_path(legacy)) {
            ensure!(
                plain_path(old).starts_with(self.root.join("sessions")),
                "原生记录属于其他 OMP 配置目录，未迁移"
            );
            return Ok(Some(original));
        }
        let _lease = lease::acquire(&self.owners, &original.id)?;
        let dir = self.project_dir(&s.cwd);
        fs::create_dir_all(&dir)?;
        let target = dir.join(old.file_name().context("无效的旧会话路径")?);
        if target.try_exists()? {
            let current = history::read(&target)?;
            ensure!(
                current.id == original.id
                    && project_key(&current.cwd) == project_key(&original.cwd),
                "迁移目标存在其他会话，已保留两份记录"
            );
            // A previous run may have published the journal before persisting
            // its GUI binding. Terminal OMP may already have added new turns.
            return Ok(Some(current));
        } else {
            // Publish the journal only after its referenced artifacts exist.
            copy_tree(&old.with_extension(""), &target.with_extension(""))?;
            let staging = target.with_extension(format!("{}.gui-sync-tmp", uuid::Uuid::new_v4()));
            let mut dst = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&staging)?;
            std::io::copy(&mut fs::File::open(old)?, &mut dst)?;
            dst.sync_all()?;
            drop(dst);
            fs::rename(&staging, &target)?;
        }
        // Retain the old private journal as a migration backup. It is never
        // scanned again once native_root has been persisted.
        history::read(&target).map(Some)
    }
    pub(crate) fn delete(&self, s: &Session, legacy: &Path) -> Result<()> {
        self.delete_checked(s, legacy, false).map(|_| ())
    }
    pub(crate) fn discard_if_untouched(&self, s: &Session, legacy: &Path) -> Result<bool> {
        if !s.is_untouched() {
            return Ok(false);
        }
        self.delete_checked(s, legacy, true)
    }
    fn delete_checked(&self, s: &Session, legacy: &Path, empty_only: bool) -> Result<bool> {
        ensure!(
            s.native_root.as_deref() == Some(self.root.to_string_lossy().as_ref()),
            "会话未完成原生同步，暂不能删除"
        );
        let id = s.native_id.as_deref().context("缺少原生会话编号")?;
        let _lease = lease::acquire(&self.owners, id)?;
        let path = Path::new(s.omp_session.as_deref().context("缺少原生会话路径")?);
        validate_path(path, &self.root.join("sessions"))?;
        if empty_only && (!path.try_exists()? || !history::untouched_gui_session(path, id)?) {
            return Ok(false);
        }
        if empty_only {
            // Backups or attachments can retain work absent from the current
            // branch. Automatic cleanup must never remove that retained data.
            for dir in [path.with_extension(""), legacy.join(&s.id)] {
                if dir.try_exists()? && fs::read_dir(dir)?.next().is_some() {
                    return Ok(false);
                }
            }
            let prefix = format!(
                "{}.",
                path.file_name().context("无效会话路径")?.to_string_lossy()
            );
            for entry in fs::read_dir(path.parent().context("会话目录无效")?)? {
                let name = entry?.file_name().to_string_lossy().into_owned();
                if name.starts_with(&prefix) && name.ends_with(".bak") {
                    return Ok(false);
                }
            }
        }
        if path.try_exists()? {
            ensure!(
                history::header(path)?["id"] == id,
                "原生会话编号已改变，未删除"
            );
        }
        // Remove retained private copies first, so a failure leaves the native
        // journal and GUI metadata available for a retry.
        let old_dir = legacy.join(&s.id);
        if old_dir.is_dir() {
            validate_path(&old_dir.join("placeholder.jsonl"), legacy)?;
            for entry in fs::read_dir(old_dir)? {
                let p = entry?.path();
                if p.extension().is_some_and(|e| e == "jsonl") && history::header(&p)?["id"] == id {
                    delete_artifacts(&p)?;
                }
            }
        }
        self.delete_native(path, id)?;
        Ok(true)
    }
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if !source.try_exists()? {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        ensure!(
            !entry.file_type()?.is_symlink(),
            "会话附件包含链接，请手动迁移"
        );
        let dst = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &dst)?;
        } else if !dst.try_exists()? {
            fs::copy(entry.path(), dst)?;
        }
    }
    Ok(())
}
fn delete_artifacts(path: &Path) -> Result<()> {
    // Callers validate ownership and root before reaching this operation.
    let artifacts = path.with_extension("");
    ensure!(
        artifacts.parent() == path.parent() && artifacts != path,
        "无效的会话附件目录"
    );
    if artifacts.try_exists()? {
        let resolved = plain_path(&fs::canonicalize(&artifacts)?);
        let parent = plain_path(&fs::canonicalize(path.parent().context("会话目录无效")?)?);
        ensure!(
            resolved.starts_with(&parent),
            "附件目录指向了会话目录之外，未删除"
        );
        fs::remove_dir_all(&artifacts)?;
    }
    let prefix = format!(
        "{}.",
        path.file_name().context("无效会话路径")?.to_string_lossy()
    );
    if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry.path().extension().is_some_and(|e| e == "bak")
            {
                fs::remove_file(entry.path())?;
            }
        }
    }
    if path.try_exists()? {
        fs::remove_file(path)?;
    }
    Ok(())
}

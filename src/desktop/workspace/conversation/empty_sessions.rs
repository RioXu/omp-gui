use super::*;

impl Workspace {
    pub(super) fn discard_empty_session_on_leave(&mut self, target: &str) -> Option<String> {
        if self.save_disabled {
            return None;
        }
        let session = self
            .session()
            .filter(|s| s.id != target && s.is_untouched())?
            .clone();
        // Stop our writer before checking the native ownership lease. A stale
        // background scan cannot reimport a journal that no longer exists.
        self.engine = None;
        self.connected = false;
        self.starting = false;
        let removed = if self.demo || session.omp_session.is_none() {
            Ok(true)
        } else {
            self.session_sync
                .store
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("OMP 会话尚未同步"))
                .and_then(|store| {
                    store.discard_if_untouched(&session, &storage::data_dir().join("omp-sessions"))
                })
        };
        match removed {
            Ok(true) => {
                self.db.remove_session(&session.id);
                None
            }
            Ok(false) => None,
            Err(e) => Some(format!("空白会话暂未清理，已保留：{e:#}")),
        }
    }
}

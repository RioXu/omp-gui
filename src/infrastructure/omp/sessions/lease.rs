//! Interoperate with OMP 18's process-owned session lease on Windows.
use anyhow::{Result, ensure};
use std::path::Path;

#[cfg(windows)]
pub(super) fn acquire(owners: &Path, id: &str) -> Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError, SetLastError},
        System::Threading::CreateMutexW,
    };
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "无法安全锁定此 OMP 会话编号"
    );
    let resolved = std::path::absolute(owners.join(id))?;
    let path = format!("{}.lock", super::plain_path(&resolved).display());
    let high = xxhash_rust::xxh64::xxh64(path.as_bytes(), 0x4f4d_502d_4c4f_434b);
    let low = xxhash_rust::xxh64::xxh64(path.as_bytes(), 0x5049_2d46_494c_454c);
    let name: Vec<u16> = format!("Global\\omp-file-lock-{high:016x}{low:016x}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // OMP uses handle existence, rather than thread ownership, as its lease.
    unsafe {
        SetLastError(0);
        let raw = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        let handle = OwnedHandle::from_raw_handle(raw);
        ensure!(
            !existed,
            "此会话正在另一 OMP 进程中使用，请先退出该终端会话"
        );
        Ok(handle)
    }
}

#[cfg(not(windows))]
pub(super) fn acquire(_owners: &Path, _id: &str) -> Result<()> {
    anyhow::bail!("当前平台尚未实现 OMP 原生会话锁");
}

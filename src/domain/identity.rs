use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub(crate) fn id() -> String {
    Uuid::new_v4().to_string()
}
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

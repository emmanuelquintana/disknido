//! DiskNido engine. Read-only analysis (scan, duplicates, backups, images,
//! cleanup candidates) plus a small set of guarded mutating operations
//! (organize, compress, recycle) that always re-validate on disk first.

pub mod backups;
pub mod catalog;
pub mod classify;
pub mod cleanup;
pub mod compress;
pub mod db;
pub mod duplicates;
pub mod fsx;
pub mod images;
pub mod ops;
pub mod organize;
pub mod query;
pub mod recycle;
pub mod safety;
pub mod scan;
pub mod volumes;

pub use rusqlite;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("cancelled")]
    Cancelled,
    /// A safety rule refused the operation. The message is user-facing.
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Disk impact profile chosen by the user (Settings → Análisis).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    Low,
    #[default]
    Balanced,
    Max,
}

/// Cooperative cancel / pause shared between a task and the UI.
#[derive(Default)]
pub struct Control {
    cancel: AtomicBool,
    pause: AtomicBool,
}

impl Control {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn set_paused(&self, p: bool) {
        self.pause.store(p, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }
    /// Blocks while paused; returns Err(Cancelled) when cancelled.
    pub fn checkpoint(&self) -> Result<()> {
        while self.is_paused() && !self.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if self.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

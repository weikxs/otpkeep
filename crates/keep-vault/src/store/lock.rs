//! Only one process writes the vault at a time (design/02-data.md §5).
//!
//! The lock is an operating-system file lock on `vault.lock`, so it is released automatically when the holder
//! exits or crashes; a stale lock file left on disk does not block anyone.

use std::fs::{File, OpenOptions, TryLockError};
use std::time::{Duration, Instant};

use super::VaultPaths;
use super::atomic::ensure_private_dir;

/// How often a waiting process retries.
const RETRY_EVERY: Duration = Duration::from_millis(50);

/// Why the lock was not obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LockError {
    /// Another process holds it and did not release it in time.
    #[error("another OTPKeep process is using the data")]
    Busy,
    /// The lock file could not be created or locked.
    #[error("the data directory cannot be locked")]
    Io,
}

/// Exclusive right to write the vault in this data directory; released when dropped.
#[derive(Debug)]
pub struct VaultLock {
    _file: File,
}

impl VaultLock {
    /// Takes the lock, waiting up to `wait` for another holder to release it.
    ///
    /// # Errors
    ///
    /// - [`LockError::Busy`] when another process still holds it after `wait`.
    /// - [`LockError::Io`] when the lock file cannot be used.
    pub fn acquire(paths: &VaultPaths, wait: Duration) -> Result<Self, LockError> {
        ensure_private_dir(paths.dir()).map_err(|_| LockError::Io)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.lock())
            .map_err(|_| LockError::Io)?;
        let deadline = Instant::now() + wait;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(RETRY_EVERY);
                }
                Err(TryLockError::WouldBlock) => return Err(LockError::Busy),
                Err(TryLockError::Error(_)) => return Err(LockError::Io),
            }
        }
    }
}

#[cfg(test)]
mod tests;

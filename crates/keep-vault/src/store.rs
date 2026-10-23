//! Storage on disk: paths, atomic replacement and the single-writer lock (design/02-data.md §3.5, §5).

mod atomic;
mod lock;

pub use atomic::{VaultPaths, replace_vault, write_private};
pub use lock::{LockError, VaultLock};

/// A fresh directory for one test. Tests run with `TMPDIR` under `/tmp/otpkeep-test` (see the Makefile).
#[cfg(test)]
pub(crate) fn test_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("vault-")
        .tempdir()
        .unwrap_or_else(|e| panic!("cannot create a test directory: {e}"))
}

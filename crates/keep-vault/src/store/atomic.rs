//! Paths in the data directory and crash-safe replacement of files.
//!
//! `vault.okv` is replaced like this:
//!
//! 1. copy the current `vault.okv` to `vault.okv.prev.tmp`, flush it, rename it to `vault.okv.prev`;
//! 2. write the new content to `vault.okv.tmp`, flush it;
//! 3. rename `vault.okv.tmp` over `vault.okv`, flush the directory.
//!
//! Every step ends in an atomic rename, so whenever the process stops, `vault.okv` exists and is either the old
//! or the new version, and `vault.okv.prev` is a complete earlier version.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const VAULT: &str = "vault.okv";
const PREV_SUFFIX: &str = ".prev";
const TMP_SUFFIX: &str = ".tmp";
const LOCK: &str = "vault.lock";
const LOCAL: &str = "local.okv";
const BACKUPS: &str = "backups";
/// File version of `vault.okv`; newer versions write `vault-v<N>.okv` (design/02-data.md §6).
const CURRENT_FILE_VERSION: u32 = 1;

/// The files in one data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultPaths {
    dir: PathBuf,
}

impl VaultPaths {
    /// Paths inside `dir`. Nothing is created until something is written.
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// The data directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `vault.okv`.
    #[must_use]
    pub fn vault(&self) -> PathBuf {
        self.dir.join(VAULT)
    }

    /// `vault.okv.prev`: the last version before the current one.
    #[must_use]
    pub fn prev(&self) -> PathBuf {
        with_suffix(&self.vault(), PREV_SUFFIX)
    }

    /// `vault.okv.tmp`: the next version while it is being written.
    #[must_use]
    pub fn tmp(&self) -> PathBuf {
        with_suffix(&self.vault(), TMP_SUFFIX)
    }

    /// `vault.lock`.
    #[must_use]
    pub fn lock(&self) -> PathBuf {
        self.dir.join(LOCK)
    }

    /// `local.okv`.
    #[must_use]
    pub fn local(&self) -> PathBuf {
        self.dir.join(LOCAL)
    }

    /// `backups/`.
    #[must_use]
    pub fn backups(&self) -> PathBuf {
        self.dir.join(BACKUPS)
    }

    /// Versions of `vault-v<N>.okv` files newer than this program's, in ascending order.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory exists but cannot be read.
    pub fn newer_vault_versions(&self) -> io::Result<Vec<u32>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut versions = Vec::new();
        for entry in entries {
            let name = entry?.file_name();
            let version = name
                .to_str()
                .and_then(|n| n.strip_prefix("vault-v"))
                .and_then(|n| n.strip_suffix(".okv"))
                .and_then(|n| n.parse::<u32>().ok());
            if let Some(version) = version.filter(|v| *v > CURRENT_FILE_VERSION) {
                versions.push(version);
            }
        }
        versions.sort_unstable();
        Ok(versions)
    }
}

/// Replaces `vault.okv` with `bytes`, keeping the current version as `vault.okv.prev`.
///
/// # Errors
///
/// Returns the I/O error; `vault.okv` is unchanged when this fails.
pub fn replace_vault(paths: &VaultPaths, bytes: &[u8]) -> io::Result<()> {
    ensure_private_dir(paths.dir())?;
    let vault = paths.vault();
    if vault.exists() {
        let prev_tmp = with_suffix(&paths.prev(), TMP_SUFFIX);
        write_synced(&prev_tmp, &fs::read(&vault)?)?;
        fs::rename(&prev_tmp, paths.prev())?;
    }
    write_synced(&paths.tmp(), bytes)?;
    fs::rename(paths.tmp(), &vault)?;
    sync_dir(paths.dir())
}

/// Atomically replaces any other private file (for example `local.okv`), without keeping a previous copy.
///
/// # Errors
///
/// Returns the I/O error; the file is unchanged when this fails.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_private_dir(dir)?;
    let tmp = with_suffix(path, TMP_SUFFIX);
    write_synced(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    sync_dir(dir)
}

/// Creates `dir` if needed and makes it accessible to the user only.
pub(crate) fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes `bytes` to a new private file and flushes it to disk.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.sync_all()
}

/// Flushes a directory, so renames inside it survive a power loss (not possible, and not needed, on Windows).
#[cfg_attr(
    not(unix),
    allow(
        clippy::unnecessary_wraps,
        reason = "fails only on Unix, where directories can be flushed"
    )
)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;

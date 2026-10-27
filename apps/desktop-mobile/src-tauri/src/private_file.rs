//! Saving an export where the person chose: readable by its owner only, and never half written. The bytes go
//! to a temporary file beside the target, which replaces the target only once it is complete on disk.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Owner read and write only.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;
/// Names tried for the temporary file before giving up.
const TEMP_TRIES: u32 = 16;

/// Writes `bytes` to `path` atomically and readable by its owner only, where the system allows it. On failure
/// the file that was there is unchanged and no temporary file is left.
///
/// # Errors
///
/// Returns the I/O error.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_private_with(path, bytes, |_| Ok(()))
}

/// [`write_private`], calling `before_rename` with the complete temporary file (tests fail there).
pub(crate) fn write_private_with(
    path: &Path,
    bytes: &[u8],
    before_rename: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let (temp, mut file) = create_temp(dir, &name.to_string_lossy())?;
    let guard = RemoveOnDrop(Some(temp));
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let temp = guard.path();
    before_rename(temp)?;
    std::fs::rename(temp, path)?;
    guard.disarm();
    sync_dir(dir)
}

/// A new private file beside the target, under a name nothing else uses.
fn create_temp(dir: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 0..TEMP_TRIES {
        let temp = dir.join(format!(
            ".{name}.{}-{seed:x}-{attempt}.tmp",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(PRIVATE_MODE);
        }
        match options.open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

/// Makes the rename itself durable.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Windows has no directory handle to flush; the rename is journaled by the file system.
#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the same signature as the Unix version"
)]
const fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Removes the temporary file unless it became the target.
struct RemoveOnDrop(Option<PathBuf>);

impl RemoveOnDrop {
    fn path(&self) -> &Path {
        self.0.as_deref().unwrap_or_else(|| Path::new(""))
    }

    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use super::{write_private, write_private_with};

    fn temp_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("app-write-")
            .tempdir()
            .unwrap()
    }

    fn names(dir: &std::path::Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn writes_a_file_only_its_owner_can_read() {
        let dir = temp_dir();
        let path = dir.path().join("links.txt");

        write_private(&path, b"otpauth://demo").unwrap();
        write_private(&path, b"again").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"again");
        assert_eq!(names(dir.path()), ["links.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_others_could_read_becomes_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.path().join("links.txt");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        write_private(&path, b"new").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn a_failure_keeps_the_old_file_and_leaves_no_temporary_one() {
        let dir = temp_dir();
        let path = dir.path().join("backup.otpkeep");
        fs::write(&path, b"old").unwrap();

        let result = write_private_with(&path, b"new", |_| Err(io::Error::other("disk full")));

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(names(dir.path()), ["backup.otpkeep"]);
    }
}

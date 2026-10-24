//! Writing exports: new files only, readable by the owner only.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::{CliError, EXIT_INVALID_INPUT, EXIT_VAULT_ERROR};

#[cfg(unix)]
const FILE_MODE: u32 = 0o600;
#[cfg(unix)]
const DIR_MODE: u32 = 0o700;

/// Fails when `path` already exists, before anything is asked or written.
pub(crate) fn ensure_absent(path: &Path) -> Result<(), CliError> {
    if path.exists() {
        return Err(exists(path));
    }
    Ok(())
}

/// Creates `path`, which must not exist, and writes `bytes`; a half-written file is removed.
pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(FILE_MODE);
    }
    let mut file = options.open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => exists(path),
        _ => cannot_write(path),
    })?;
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(cannot_write(path));
    }
    Ok(())
}

/// Creates the folder `path`, which must not exist.
pub(crate) fn create_new_dir(path: &Path) -> Result<(), CliError> {
    let builder = fs::DirBuilder::new();
    // Owner-only folders on Unix; Windows has no mode bits to set.
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(DIR_MODE);
        builder
    };
    builder.create(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => exists(path),
        _ => cannot_write(path),
    })
}

fn exists(path: &Path) -> CliError {
    CliError::new(
        EXIT_INVALID_INPUT,
        "INVALID_INPUT",
        &format!(
            "'{}' already exists; choose a new name, nothing is overwritten",
            path.display()
        ),
    )
}

fn cannot_write(path: &Path) -> CliError {
    CliError::new(
        EXIT_VAULT_ERROR,
        "VAULT_ERROR",
        &format!("cannot write '{}'", path.display()),
    )
}

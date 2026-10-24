//! Reading an import file for the command line: the format is recognised by `keep_transfer::source`; this
//! module asks for a backup password when one is needed and words what was skipped.

use keep_transfer::backup::BackupError;
use keep_transfer::import::Candidate;
use keep_transfer::source::{Place, SkipReason, SourceError, read_source as read_any};
use zeroize::Zeroizing;

use crate::{CliError, EXIT_AUTH_FAILED, EXIT_INVALID_INPUT};

/// The accounts of a file, and what had to be skipped.
pub(crate) struct Source {
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) skipped: Vec<String>,
}

/// Reads `bytes` as whichever format they are; `backup_password` is asked only for a backup.
pub(crate) fn read_source(
    bytes: &[u8],
    backup_password: impl FnOnce() -> Result<Zeroizing<String>, CliError>,
) -> Result<Source, CliError> {
    let source = match read_any(bytes, None) {
        Err(SourceError::NeedsPassword) => {
            let password = backup_password()?;
            read_any(bytes, Some(&password))
        }
        other => other,
    }
    .map_err(source_error)?;
    if source.candidates.is_empty() && !source.skipped.is_empty() {
        return Err(CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "no account could be read from this file",
        ));
    }
    let skipped = source
        .skipped
        .iter()
        .map(|s| format!("skipped {}: {}", place(&s.place), reason(&s.reason)))
        .collect();
    Ok(Source {
        candidates: source.candidates,
        skipped,
    })
}

fn place(place: &Place) -> String {
    match place {
        Place::Line(line) => format!("line {line}"),
        Place::TransferAccount { code, index } => {
            format!("account {index} of transfer code {code}")
        }
        Place::Named(name) => format!("'{name}'"),
    }
}

fn reason(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NotALink => "not an otpauth:// link".to_owned(),
        SkipReason::Link(fault) => fault.to_string(),
        SkipReason::Transfer(fault) => fault.to_string(),
        SkipReason::TransferAccount(unsupported) => format!("{unsupported:?}"),
    }
}

fn source_error(error: SourceError) -> CliError {
    match error {
        SourceError::Backup(BackupError::WrongPassword | BackupError::EmptyPassword) => {
            CliError::new(EXIT_AUTH_FAILED, "AUTH_FAILED", &error.to_string())
        }
        other => CliError::new(EXIT_INVALID_INPUT, "INVALID_INPUT", &other.to_string()),
    }
}

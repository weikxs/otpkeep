//! Recognising an import file and reading its accounts (design/05-transfer.md §1).

use keep_keyuri::{SheetProblemKind, read_sheet};
use keep_transfer::backup::{BackupError, read_backup};
use keep_transfer::goldenpassport::{GpRead, read_accounts_json, read_secrets};
use keep_transfer::import::{Candidate, from_migration, from_sheet};
use keep_transfer::migration::decode_migration_link;
use keep_transfer::qr::{QrError, decode_qr_image};
use zeroize::Zeroizing;

use crate::{CliError, EXIT_AUTH_FAILED, EXIT_INVALID_INPUT};

const SEALED_MAGIC: &[u8] = b"OKV\0";
const BINARY_PLIST_MAGIC: &[u8] = b"bplist";
const TRANSFER_SCHEME: &str = "otpauth-migration://";

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
    if bytes.starts_with(SEALED_MAGIC) {
        let password = backup_password()?;
        let candidates = read_backup(bytes, &password).map_err(backup_error)?;
        return Ok(Source {
            candidates,
            skipped: Vec::new(),
        });
    }
    if bytes.starts_with(BINARY_PLIST_MAGIC) {
        return read_secrets(bytes)
            .map(goldenpassport)
            .map_err(|_| unknown());
    }
    match decode_qr_image(bytes) {
        Ok(texts) => return Ok(from_texts(texts.iter().map(|t| t.as_str()))),
        Err(QrError::TooLarge) => {
            return Err(CliError::new(
                EXIT_INVALID_INPUT,
                "INVALID_INPUT",
                "the image is too large to read",
            ));
        }
        Err(QrError::NotAnImage | QrError::TooLong) => {}
    }
    let text = std::str::from_utf8(bytes).map_err(|_| unknown())?;
    if text.trim_start().starts_with(['[', '{']) {
        return read_accounts_json(bytes)
            .map(goldenpassport)
            .map_err(|_| unknown());
    }
    let source = from_texts(text.lines());
    if source.candidates.is_empty() && !source.skipped.is_empty() {
        return Err(CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "no account could be read from this file",
        ));
    }
    Ok(source)
}

fn goldenpassport(read: GpRead) -> Source {
    Source {
        candidates: read.candidates,
        skipped: read
            .problems
            .iter()
            .map(|p| format!("skipped '{}': {}", p.name, p.fault))
            .collect(),
    }
}

/// Lines of a text file or texts of QR codes: transfer codes, and links (with `# title` comments) otherwise.
fn from_texts<'a>(texts: impl Iterator<Item = &'a str>) -> Source {
    let mut sheet = Zeroizing::new(String::new());
    let mut batches = Vec::new();
    let mut skipped = Vec::new();
    for (index, text) in texts.enumerate() {
        let line = text.trim();
        if line.starts_with(TRANSFER_SCHEME) {
            match decode_migration_link(line) {
                Ok(batch) => batches.push(batch),
                Err(fault) => skipped.push(format!("skipped line {}: {fault}", index + 1)),
            }
            sheet.push('\n');
            continue;
        }
        sheet.push_str(line);
        sheet.push('\n');
    }
    let read = read_sheet(&sheet);
    for problem in &read.problems {
        let reason = match &problem.kind {
            SheetProblemKind::NotALink => "not an otpauth:// link".to_owned(),
            SheetProblemKind::BadLink(fault) => fault.to_string(),
        };
        skipped.push(format!("skipped line {}: {reason}", problem.line));
    }
    for batch in &batches {
        for problem in &batch.unreadable {
            skipped.push(format!(
                "skipped account {} of transfer code {}: {:?}",
                problem.index + 1,
                batch.batch_index + 1,
                problem.reason
            ));
        }
    }
    let mut candidates = from_sheet(&read);
    candidates.extend(from_migration(&batches));
    Source {
        candidates,
        skipped,
    }
}

fn backup_error(error: BackupError) -> CliError {
    match error {
        BackupError::WrongPassword | BackupError::EmptyPassword => {
            CliError::new(EXIT_AUTH_FAILED, "AUTH_FAILED", &error.to_string())
        }
        other => CliError::new(EXIT_INVALID_INPUT, "INVALID_INPUT", &other.to_string()),
    }
}

fn unknown() -> CliError {
    CliError::new(
        EXIT_INVALID_INPUT,
        "INVALID_INPUT",
        "OTPKeep cannot read this file; expected an OTPKeep backup, a GoldenPassport export, \
         a list of otpauth:// links or a QR code image",
    )
}

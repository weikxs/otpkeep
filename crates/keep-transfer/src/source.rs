//! Recognising what an import source is and reading its accounts (design/05-transfer.md §1): an OTPKeep
//! backup, GoldenPassport data, a QR code image, a text of `otpauth://` links or Google transfer codes.
//! Shared by every way in: files, pasted text and pictures.

use keep_keyuri::{KeyUriFault, SheetProblemKind, read_sheet};
use zeroize::Zeroizing;

use crate::backup::{BackupError, read_backup};
use crate::goldenpassport::{GpRead, read_accounts_json, read_secrets};
use crate::import::{Candidate, from_migration, from_sheet};
use crate::migration::{MigrationFault, Unsupported, decode_migration_link};
use crate::qr::{QrError, decode_qr_image};

const SEALED_MAGIC: &[u8] = b"OKV\0";
const BINARY_PLIST_MAGIC: &[u8] = b"bplist";
const TRANSFER_SCHEME: &str = "otpauth-migration://";

/// The accounts of a source, and what had to be left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Accounts for the import preview.
    pub candidates: Vec<Candidate>,
    /// What could not be read, in source order.
    pub skipped: Vec<Skipped>,
}

/// One part of a source that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Where it is.
    pub place: Place,
    /// Why it was left out.
    pub reason: SkipReason,
}

/// Where a skipped part is in its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A line of a text, or the n-th QR code of a picture (from 1).
    Line(usize),
    /// Account `index` of transfer code `code` (both from 1).
    TransferAccount {
        /// The transfer code.
        code: u32,
        /// The account in it.
        index: usize,
    },
    /// A GoldenPassport account, by its name.
    Named(String),
}

/// Why a part was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Text that is not a link at all.
    NotALink,
    /// A link that cannot be used.
    Link(KeyUriFault),
    /// A Google transfer code that cannot be read.
    Transfer(MigrationFault),
    /// An account in a transfer code with values OTPKeep does not know.
    TransferAccount(Unsupported),
}

/// Why a source could not be read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// It is an OTPKeep backup; read it again with its password.
    #[error("this is an OTPKeep backup; its password is needed")]
    NeedsPassword,
    /// The backup could not be opened.
    #[error(transparent)]
    Backup(BackupError),
    /// A picture too large to search for codes.
    #[error("the image is too large to read")]
    ImageTooLarge,
    /// Not a format OTPKeep knows.
    #[error(
        "OTPKeep cannot read this; expected an OTPKeep backup, GoldenPassport data, a list of \
         otpauth:// links or a QR code image"
    )]
    Unknown,
}

/// Reads `bytes` as whichever format they are. A backup needs `backup_password`; without one the answer is
/// [`SourceError::NeedsPassword`], so the caller asks only when it is needed.
///
/// # Errors
///
/// Returns [`SourceError`] when nothing of the source can be read.
pub fn read_source(bytes: &[u8], backup_password: Option<&str>) -> Result<Source, SourceError> {
    if bytes.starts_with(SEALED_MAGIC) {
        let password = backup_password.ok_or(SourceError::NeedsPassword)?;
        let candidates = read_backup(bytes, password).map_err(SourceError::Backup)?;
        return Ok(Source {
            candidates,
            skipped: Vec::new(),
        });
    }
    if bytes.starts_with(BINARY_PLIST_MAGIC) {
        return read_secrets(bytes)
            .map(goldenpassport)
            .map_err(|_| SourceError::Unknown);
    }
    match decode_qr_image(bytes) {
        Ok(texts) => return Ok(read_texts(texts.iter().map(|t| t.as_str()))),
        Err(QrError::TooLarge) => return Err(SourceError::ImageTooLarge),
        Err(QrError::NotAnImage | QrError::TooLong) => {}
    }
    let text = std::str::from_utf8(bytes).map_err(|_| SourceError::Unknown)?;
    if text.trim_start().starts_with(['[', '{']) {
        return read_accounts_json(bytes)
            .map(goldenpassport)
            .map_err(|_| SourceError::Unknown);
    }
    Ok(read_text(text))
}

/// Reads pasted or typed text: one link, several lines of links (with `# title` comments) or transfer codes.
#[must_use]
pub fn read_text(text: &str) -> Source {
    read_texts(text.lines())
}

fn goldenpassport(read: GpRead) -> Source {
    Source {
        candidates: read.candidates,
        skipped: read
            .problems
            .into_iter()
            .map(|p| Skipped {
                place: Place::Named(p.name),
                reason: SkipReason::Link(p.fault),
            })
            .collect(),
    }
}

/// Lines of a text or texts of QR codes: transfer codes where they appear, links otherwise.
fn read_texts<'a>(texts: impl Iterator<Item = &'a str>) -> Source {
    let mut sheet = Zeroizing::new(String::new());
    let mut batches = Vec::new();
    let mut skipped = Vec::new();
    for (index, text) in texts.enumerate() {
        // Transfer codes are read here; their lines stay empty in the sheet so line numbers still match.
        if let Some(code) = transfer_code(text) {
            match decode_migration_link(code) {
                Ok(batch) => batches.push(batch),
                Err(fault) => skipped.push(Skipped {
                    place: Place::Line(index + 1),
                    reason: SkipReason::Transfer(fault),
                }),
            }
        } else {
            sheet.push_str(text);
        }
        sheet.push('\n');
    }
    let read = read_sheet(&sheet);
    skipped.extend(read.problems.iter().map(|problem| Skipped {
        place: Place::Line(problem.line),
        reason: match &problem.kind {
            SheetProblemKind::NotALink => SkipReason::NotALink,
            SheetProblemKind::BadLink(fault) => SkipReason::Link(fault.clone()),
        },
    }));
    for batch in &batches {
        skipped.extend(batch.unreadable.iter().map(|problem| Skipped {
            place: Place::TransferAccount {
                code: batch.batch_index + 1,
                index: problem.index + 1,
            },
            reason: SkipReason::TransferAccount(problem.reason),
        }));
    }
    skipped.sort_by_key(|s| match s.place {
        Place::Line(line) => line,
        _ => usize::MAX,
    });
    let mut candidates = from_sheet(&read);
    candidates.extend(from_migration(&batches));
    Source {
        candidates,
        skipped,
    }
}

/// The transfer code in a line, also with words around it.
fn transfer_code(line: &str) -> Option<&str> {
    let start = line.to_ascii_lowercase().find(TRANSFER_SCHEME)?;
    let code = &line[start..];
    Some(code.split_whitespace().next().unwrap_or(code))
}

#[cfg(test)]
mod tests;

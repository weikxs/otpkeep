//! Changes to the vault as pure functions: each takes a document and returns a new one, so a failed save or
//! a rejected edit never leaves a half-changed vault (design/02-data.md, design/03-passcode.md §5).
//!
//! Every change stamps the fields it touches with a new hybrid logical clock value, later than anything the
//! document has seen, so sync can merge field by field (design/06-sync.md §6).

use keep_passcode::{CodeSpec, SecretKey};

use crate::model::{Entry, EntryId, Extra, Field, Hlc, KeyMaterial, VaultDoc};
use crate::rank;

mod lifecycle;
mod order;

pub use lifecycle::{
    TRASH_RETENTION_MS, move_to_trash, move_to_trash_mut, next_hotp, old_key_copy, purge,
    purge_expired_trash, replace_key, restore,
};
pub use order::{Placement, move_entry, ordered, trashed};

/// Longest display name (PRD FR-3.1).
pub const TITLE_MAX_CHARS: usize = 100;
/// Longest note (PRD FR-3.1).
pub const NOTE_MAX_CHARS: usize = 2000;
/// Longest tag (PRD FR-3.1).
pub const TAG_MAX_CHARS: usize = 30;
/// Most tags on one entry (PRD FR-3.1).
pub const TAGS_MAX: usize = 20;
/// Longest issuer or account name (design/10-reference.md §2).
pub const NAME_MAX_CHARS: usize = 256;

/// Why a change was rejected. The document is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// No entry with that id.
    #[error("the entry does not exist")]
    NotFound,
    /// The secret is already in the vault (including the trash); add it anyway with `keep_duplicate`.
    #[error("this secret is already in the vault")]
    Duplicate {
        /// The entry that has it.
        existing: EntryId,
    },
    /// The display name is empty.
    #[error("the name cannot be empty")]
    TitleEmpty,
    /// The display name is too long.
    #[error("the name is longer than {TITLE_MAX_CHARS} characters")]
    TitleTooLong,
    /// The issuer is too long.
    #[error("the service name is longer than {NAME_MAX_CHARS} characters")]
    IssuerTooLong,
    /// The account is too long.
    #[error("the account name is longer than {NAME_MAX_CHARS} characters")]
    AccountTooLong,
    /// The note is too long.
    #[error("the note is longer than {NOTE_MAX_CHARS} characters")]
    NoteTooLong,
    /// Too many tags.
    #[error("an entry has at most {TAGS_MAX} tags")]
    TooManyTags,
    /// A tag is too long.
    #[error("a tag is longer than {TAG_MAX_CHARS} characters")]
    TagTooLong,
    /// Only HOTP entries have a counter to step.
    #[error("only counter-based entries have a next code")]
    NotHotp,
}

/// A new entry as entered by the user or read from an import.
#[derive(Debug, Clone)]
pub struct NewEntry {
    /// Display name.
    pub title: String,
    /// Service name.
    pub issuer: String,
    /// Account name.
    pub account: String,
    /// The secret.
    pub secret: SecretKey,
    /// How codes are computed.
    pub spec: CodeSpec,
    /// HOTP counter; 0 for TOTP.
    pub counter: u64,
    /// Note.
    pub note: String,
    /// Tags.
    pub tags: Vec<String>,
    /// "Add anyway" for a secret that is already in the vault.
    pub keep_duplicate: bool,
}

/// One user-visible change to an entry's descriptive fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// New display name.
    Title(String),
    /// New service name.
    Issuer(String),
    /// New account name.
    Account(String),
    /// New note.
    Note(String),
    /// New tags.
    Tags(Vec<String>),
    /// Pinned or not.
    Pinned(bool),
}

/// Adds an entry at the end of the list.
///
/// # Errors
///
/// Returns [`EditError`] for invalid fields or a duplicate secret without `keep_duplicate`.
pub fn add_entry(
    doc: &VaultDoc,
    new: NewEntry,
    now_ms: u64,
) -> Result<(VaultDoc, EntryId), EditError> {
    add_entry_with_id(doc, new, EntryId::random(), now_ms)
}

/// [`add_entry`] with a given id, for reproducible simulations and tests.
///
/// # Errors
///
/// Returns [`EditError`] like [`add_entry`], and [`EditError::Duplicate`] when the id is taken.
pub fn add_entry_with_id(
    doc: &VaultDoc,
    new: NewEntry,
    id: EntryId,
    now_ms: u64,
) -> Result<(VaultDoc, EntryId), EditError> {
    if doc.entries.iter().any(|e| e.id == id) {
        return Err(EditError::Duplicate { existing: id });
    }
    let title = title(&new.title)?;
    let issuer = name(&new.issuer, EditError::IssuerTooLong)?;
    let account = name(&new.account, EditError::AccountTooLong)?;
    let note = note(&new.note)?;
    let tags = tags(&new.tags)?;
    if !new.keep_duplicate
        && let Some(existing) = find_duplicate(doc, &new.secret)
    {
        return Err(EditError::Duplicate { existing });
    }
    let stamp = next_stamp(doc, now_ms);
    let rank = order::last_rank(doc).map_or_else(rank::first, |last| rank::after(&last));
    let field = |value| Field { value, stamp };
    let entry = Entry {
        id,
        created_at: stamp,
        title: field(title),
        issuer: field(issuer),
        account: field(account),
        key: Field {
            value: KeyMaterial {
                secret: new.secret,
                spec: new.spec,
                counter: new.counter,
                key_id: stamp,
            },
            stamp,
        },
        note: field(note),
        tags: Field { value: tags, stamp },
        rank: field(rank),
        pinned: Field {
            value: false,
            stamp,
        },
        keep_duplicate: Field {
            value: new.keep_duplicate,
            stamp,
        },
        trashed_at: Field { value: None, stamp },
        old_key_of: Field { value: None, stamp },
        extra: Extra::new(),
    };
    let id = entry.id;
    let mut entries = doc.entries.clone();
    entries.push(entry);
    Ok((
        VaultDoc {
            entries,
            clock: stamp,
            ..doc.clone()
        },
        id,
    ))
}

/// Applies one change to an entry's descriptive fields.
///
/// # Errors
///
/// Returns [`EditError`] for an unknown entry or an invalid value.
pub fn edit_entry(
    doc: &VaultDoc,
    id: EntryId,
    change: Change,
    now_ms: u64,
) -> Result<VaultDoc, EditError> {
    edit_entry_all(doc, id, vec![change], now_ms)
}

/// Applies several changes to one entry's descriptive fields as one change: one new stamp for all of them, so
/// an editor's Save is saved once. Every value is checked before anything changes.
///
/// # Errors
///
/// Returns [`EditError`] for an unknown entry or an invalid value.
pub fn edit_entry_all(
    doc: &VaultDoc,
    id: EntryId,
    changes: Vec<Change>,
    now_ms: u64,
) -> Result<VaultDoc, EditError> {
    update_entry(doc, id, now_ms, |entry, stamp| {
        changes
            .into_iter()
            .try_fold(entry.clone(), |entry, change| applied(entry, change, stamp))
    })
}

/// [`edit_entry`] on a copy the caller owns, for many changes in a row without copying the vault each time.
/// On failure `doc` is unchanged.
///
/// # Errors
///
/// Returns [`EditError`] for an unknown entry or an invalid value.
pub fn edit_entry_mut(
    doc: &mut VaultDoc,
    id: EntryId,
    change: Change,
    now_ms: u64,
) -> Result<(), EditError> {
    update_entry_mut(doc, id, now_ms, |entry, stamp| {
        applied(entry.clone(), change, stamp)
    })
}

/// `entry` with `change` applied and stamped.
fn applied(mut entry: Entry, change: Change, stamp: Hlc) -> Result<Entry, EditError> {
    match change {
        Change::Title(value) => {
            entry.title = Field {
                value: title(&value)?,
                stamp,
            }
        }
        Change::Issuer(value) => {
            entry.issuer = Field {
                value: name(&value, EditError::IssuerTooLong)?,
                stamp,
            }
        }
        Change::Account(value) => {
            entry.account = Field {
                value: name(&value, EditError::AccountTooLong)?,
                stamp,
            };
        }
        Change::Note(value) => {
            entry.note = Field {
                value: note(&value)?,
                stamp,
            }
        }
        Change::Tags(value) => {
            entry.tags = Field {
                value: tags(&value)?,
                stamp,
            }
        }
        Change::Pinned(value) => entry.pinned = Field { value, stamp },
    }
    Ok(entry)
}

/// The entry that already holds `secret`, including entries in the trash. Only the secret bytes count, not
/// the parameters (PRD FR-2.9).
#[must_use]
pub fn find_duplicate(doc: &VaultDoc, secret: &SecretKey) -> Option<EntryId> {
    doc.entries
        .iter()
        .find(|e| e.key.value.secret.expose() == secret.expose())
        .map(|e| e.id)
}

/// A timestamp later than anything `doc` has seen, on this device.
pub(crate) fn next_stamp(doc: &VaultDoc, now_ms: u64) -> Hlc {
    let wall_ms = now_ms.max(doc.clock.wall_ms);
    let logical = if wall_ms == doc.clock.wall_ms {
        doc.clock.logical.saturating_add(1)
    } else {
        0
    };
    Hlc {
        wall_ms,
        logical,
        device: doc.device_tag,
    }
}

/// Replaces one entry with what `change` returns, stamping with a new timestamp.
pub(crate) fn update_entry(
    doc: &VaultDoc,
    id: EntryId,
    now_ms: u64,
    change: impl FnOnce(&Entry, Hlc) -> Result<Entry, EditError>,
) -> Result<VaultDoc, EditError> {
    let mut next = doc.clone();
    update_entry_mut(&mut next, id, now_ms, change)?;
    Ok(next)
}

/// [`update_entry`] in a document the caller owns. The old entry is dropped (and its secret wiped) only once
/// the new one is ready, so on failure `doc` is unchanged.
pub(crate) fn update_entry_mut(
    doc: &mut VaultDoc,
    id: EntryId,
    now_ms: u64,
    change: impl FnOnce(&Entry, Hlc) -> Result<Entry, EditError>,
) -> Result<(), EditError> {
    let index = doc
        .entries
        .iter()
        .position(|e| e.id == id)
        .ok_or(EditError::NotFound)?;
    let stamp = next_stamp(doc, now_ms);
    doc.entries[index] = change(&doc.entries[index], stamp)?;
    doc.clock = stamp;
    Ok(())
}

fn title(value: &str) -> Result<String, EditError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(EditError::TitleEmpty);
    }
    limited(value, TITLE_MAX_CHARS, EditError::TitleTooLong)
}

fn name(value: &str, too_long: EditError) -> Result<String, EditError> {
    limited(value.trim(), NAME_MAX_CHARS, too_long)
}

fn note(value: &str) -> Result<String, EditError> {
    limited(value, NOTE_MAX_CHARS, EditError::NoteTooLong)
}

/// Trims tags, drops empty ones and keeps the first of tags that differ only in case.
fn tags(values: &[String]) -> Result<Vec<String>, EditError> {
    let mut kept: Vec<String> = Vec::new();
    for tag in values.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let tag = limited(tag, TAG_MAX_CHARS, EditError::TagTooLong)?;
        if !kept.iter().any(|k| k.to_lowercase() == tag.to_lowercase()) {
            kept.push(tag);
        }
    }
    if kept.len() > TAGS_MAX {
        return Err(EditError::TooManyTags);
    }
    Ok(kept)
}

fn limited(value: &str, max_chars: usize, too_long: EditError) -> Result<String, EditError> {
    if value.chars().count() > max_chars {
        return Err(too_long);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests;

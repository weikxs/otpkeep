//! The import preview every import goes through (PRD FR-6.6, design/05-transfer.md §1).
//!
//! Imports only add entries: nothing existing is changed or removed. Values longer than OTPKeep allows are cut
//! in the preview, with a note, so the final commit cannot fail half-way through.

use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::edit::{
    EditError, NAME_MAX_CHARS, NOTE_MAX_CHARS, NewEntry, TAG_MAX_CHARS, TAGS_MAX, TITLE_MAX_CHARS,
    add_entry, find_duplicate,
};
use keep_vault::{EntryId, Hlc, VaultDoc};

mod sources;

pub use sources::{from_link, from_migration, from_sheet};

/// Name used when an account has no name, no service and no account name.
const FALLBACK_TITLE: &str = "OTP";

/// Where a candidate came from in its source, for messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The n-th account (from 0).
    Item(usize),
    /// A line of a text file (from 1).
    Line(usize),
    /// Account `index` of transfer code `code` (both from 0).
    Code {
        /// Code position in the export.
        code: u32,
        /// Account position in the code.
        index: usize,
    },
}

/// One account read from an import source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Display name; may be empty, then the service or account name is used.
    pub title: String,
    /// Service name.
    pub issuer: String,
    /// Account name.
    pub account: String,
    /// The secret.
    pub secret: SecretKey,
    /// How codes are computed.
    pub spec: CodeSpec,
    /// HOTP counter.
    pub counter: u64,
    /// Note.
    pub note: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Where it came from.
    pub origin: Origin,
}

/// Something the preview had to cut to fit OTPKeep's limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// The name was longer than allowed.
    TitleShortened,
    /// The service name was longer than allowed.
    IssuerShortened,
    /// The account name was longer than allowed.
    AccountShortened,
    /// The note was longer than allowed.
    NoteShortened,
    /// Some tags were too long and were shortened.
    TagsShortened,
    /// There were more tags than allowed; the rest were dropped.
    TagsDropped,
}

/// A candidate whose secret is not in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fresh {
    /// The fitted candidate.
    pub candidate: Candidate,
    /// Another entry already has this name (ignoring case); the user may want to rename it.
    pub title_taken: bool,
    /// What was cut to fit.
    pub lost: Vec<Lost>,
    position: usize,
}

/// A candidate whose secret is already present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    /// The fitted candidate.
    pub candidate: Candidate,
    /// The vault entry (including the trash) that has the secret.
    pub existing: Option<EntryId>,
    /// The earlier fresh candidate of this import that has the secret.
    pub same_as_fresh: Option<usize>,
    /// What was cut to fit.
    pub lost: Vec<Lost>,
    position: usize,
}

/// What an import would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    /// New accounts; selected by default.
    pub fresh: Vec<Fresh>,
    /// Accounts whose secret is already present; not selected by default.
    pub duplicates: Vec<Duplicate>,
    based_on: Hlc,
}

/// Which accounts to import, by position in [`ImportPreview::fresh`] and [`ImportPreview::duplicates`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// One flag per fresh candidate.
    pub fresh: Vec<bool>,
    /// One flag per duplicate; a chosen duplicate is added and marked "keep duplicate".
    pub duplicates: Vec<bool>,
}

impl Choice {
    /// The default: every fresh candidate, no duplicates.
    #[must_use]
    pub fn all(preview: &ImportPreview) -> Self {
        Self {
            fresh: vec![true; preview.fresh.len()],
            duplicates: vec![false; preview.duplicates.len()],
        }
    }
}

/// Why an import was not committed. Nothing was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The choice does not match the preview.
    #[error("the selection does not match the preview")]
    ChoiceMismatch,
    /// The vault changed after the preview; preview again.
    #[error("the vault changed since the preview; please review the import again")]
    VaultChanged,
    /// An entry was rejected.
    #[error(transparent)]
    Edit(#[from] EditError),
}

/// Sorts candidates into new accounts and duplicates, and fits them to OTPKeep's limits.
#[must_use]
pub fn preview(candidates: Vec<Candidate>, doc: &VaultDoc) -> ImportPreview {
    let mut fresh: Vec<Fresh> = Vec::new();
    let mut duplicates = Vec::new();
    for (position, candidate) in candidates.into_iter().enumerate() {
        let (candidate, lost) = fit(candidate);
        let existing = find_duplicate(doc, &candidate.secret);
        let same_as_fresh = fresh
            .iter()
            .position(|f| f.candidate.secret.expose() == candidate.secret.expose());
        if existing.is_some() || same_as_fresh.is_some() {
            duplicates.push(Duplicate {
                candidate,
                existing,
                same_as_fresh,
                lost,
                position,
            });
            continue;
        }
        let title = candidate.title.to_lowercase();
        let title_taken = doc
            .entries
            .iter()
            .any(|e| e.title.value.to_lowercase() == title);
        fresh.push(Fresh {
            candidate,
            title_taken,
            lost,
            position,
        });
    }
    ImportPreview {
        fresh,
        duplicates,
        based_on: doc.clock,
    }
}

/// Adds the chosen candidates, in their original order, all or nothing.
///
/// # Errors
///
/// Returns [`ImportError`]; the document is unchanged on failure.
pub fn commit_import(
    doc: &VaultDoc,
    preview: &ImportPreview,
    choice: &Choice,
    now_ms: u64,
) -> Result<(VaultDoc, Vec<EntryId>), ImportError> {
    if choice.fresh.len() != preview.fresh.len()
        || choice.duplicates.len() != preview.duplicates.len()
    {
        return Err(ImportError::ChoiceMismatch);
    }
    if doc.clock != preview.based_on {
        return Err(ImportError::VaultChanged);
    }
    let mut chosen: Vec<(usize, &Candidate, bool)> = Vec::new();
    chosen.extend(
        preview
            .fresh
            .iter()
            .zip(&choice.fresh)
            .filter(|(_, c)| **c)
            .map(|(f, _)| (f.position, &f.candidate, false)),
    );
    chosen.extend(
        preview
            .duplicates
            .iter()
            .zip(&choice.duplicates)
            .filter(|(_, c)| **c)
            .map(|(d, _)| (d.position, &d.candidate, true)),
    );
    chosen.sort_by_key(|(position, _, _)| *position);
    let mut current = doc.clone();
    let mut added = Vec::with_capacity(chosen.len());
    for (_, candidate, keep_duplicate) in chosen {
        let (next, id) = add_entry(&current, new_entry(candidate, keep_duplicate), now_ms)?;
        current = next;
        added.push(id);
    }
    tracing::info!(added = added.len(), "import committed");
    Ok((current, added))
}

fn new_entry(candidate: &Candidate, keep_duplicate: bool) -> NewEntry {
    NewEntry {
        title: candidate.title.clone(),
        issuer: candidate.issuer.clone(),
        account: candidate.account.clone(),
        secret: candidate.secret.clone(),
        spec: candidate.spec,
        counter: candidate.counter,
        note: candidate.note.clone(),
        tags: candidate.tags.clone(),
        keep_duplicate,
    }
}

/// Names an untitled candidate and cuts values to OTPKeep's limits.
fn fit(candidate: Candidate) -> (Candidate, Vec<Lost>) {
    let mut lost = Vec::new();
    let mut cut = |text: &str, max: usize, what: Lost| {
        let text = text.trim();
        if text.chars().count() > max {
            lost.push(what);
            text.chars()
                .take(max)
                .collect::<String>()
                .trim_end()
                .to_owned()
        } else {
            text.to_owned()
        }
    };
    let issuer = cut(&candidate.issuer, NAME_MAX_CHARS, Lost::IssuerShortened);
    let account = cut(&candidate.account, NAME_MAX_CHARS, Lost::AccountShortened);
    let named = [candidate.title.trim(), issuer.as_str(), account.as_str()]
        .into_iter()
        .find(|t| !t.is_empty())
        .unwrap_or(FALLBACK_TITLE)
        .to_owned();
    let title = cut(&named, TITLE_MAX_CHARS, Lost::TitleShortened);
    let note = if candidate.note.chars().count() > NOTE_MAX_CHARS {
        lost.push(Lost::NoteShortened);
        candidate.note.chars().take(NOTE_MAX_CHARS).collect()
    } else {
        candidate.note.clone()
    };
    let mut tags: Vec<String> = candidate
        .tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    if tags.iter().any(|t| t.chars().count() > TAG_MAX_CHARS) {
        lost.push(Lost::TagsShortened);
        tags = tags
            .into_iter()
            .map(|t| t.chars().take(TAG_MAX_CHARS).collect())
            .collect();
    }
    if tags.len() > TAGS_MAX {
        lost.push(Lost::TagsDropped);
        tags.truncate(TAGS_MAX);
    }
    // Keep the order of `lost` stable for callers: title, issuer, account, note, tags.
    lost.sort_by_key(|l| *l as u8);
    (
        Candidate {
            title,
            issuer,
            account,
            note,
            tags,
            ..candidate
        },
        lost,
    )
}

#[cfg(test)]
mod tests;

//! What the window sees of a staged import: display fields, a current code to compare with the website
//! (PRD FR-2.2), why parts were skipped. Never a secret.

use keep_keyuri::KeyUriFault;
use keep_passcode::{CodeKind, CodeSpec, Digest, Moment, derive_passcode};
use keep_transfer::import::{Candidate, Lost};
use keep_transfer::source::{Place, SkipReason, Skipped};
use keep_vault::VaultDoc;
use serde::Serialize;

use crate::staging::Staged;

const MS_PER_S: u64 = 1000;

/// A staged import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportView {
    /// Names this import in the commit.
    pub token: String,
    /// New accounts first, then accounts already in the vault.
    pub rows: Vec<ImportRow>,
    pub skipped: Vec<SkippedView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowGroup {
    /// Selected by default.
    Fresh,
    /// The secret is already in the vault, or earlier in this import; not selected by default.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    pub group: RowGroup,
    /// Position within its group, as the commit's choice counts.
    pub index: usize,
    pub title: String,
    pub issuer: String,
    pub account: String,
    /// `totp`, `hotp` or `steam`.
    pub kind: &'static str,
    pub algorithm: &'static str,
    pub digits: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_s: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter: Option<u64>,
    /// The code right now, to compare with the website; not for counter codes, which would move on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_code: Option<String>,
    /// Another entry already has this name.
    pub title_taken: bool,
    /// What was cut to fit OTPKeep's limits.
    pub lost: Vec<&'static str>,
    /// The entry that already has this secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_title: Option<String>,
    /// The earlier new row of this import with the same secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_as_row: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PlaceView {
    Line { line: usize },
    Transfer { code: u32, index: usize },
    Named { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedView {
    pub place: PlaceView,
    /// A code the window words (`NOT_A_LINK`, `MICROSOFT_PUSH`, `SECRET_INVALID`, …).
    pub reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The view of `staged` against `doc` (for the names of existing entries) at `now_ms`.
#[must_use]
pub fn view(token: &str, staged: &Staged, doc: &VaultDoc, now_ms: u64) -> ImportView {
    let fresh = staged
        .preview
        .fresh
        .iter()
        .enumerate()
        .map(|(index, f)| ImportRow {
            title_taken: f.title_taken,
            lost: f.lost.iter().map(|l| lost_code(*l)).collect(),
            ..row(RowGroup::Fresh, index, &f.candidate, now_ms)
        });
    let duplicates = staged
        .preview
        .duplicates
        .iter()
        .enumerate()
        .map(|(index, d)| ImportRow {
            lost: d.lost.iter().map(|l| lost_code(*l)).collect(),
            existing_title: d.existing.and_then(|id| {
                doc.entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.title.value.clone())
            }),
            same_as_row: d.same_as_fresh,
            ..row(RowGroup::Duplicate, index, &d.candidate, now_ms)
        });
    ImportView {
        token: token.to_owned(),
        rows: fresh.chain(duplicates).collect(),
        skipped: staged.skipped.iter().map(skipped_view).collect(),
    }
}

fn row(group: RowGroup, index: usize, candidate: &Candidate, now_ms: u64) -> ImportRow {
    let spec = candidate.spec;
    let (kind, period_s, counter) = match spec.kind() {
        CodeKind::TimeBased { period_s } if spec.is_steam() => ("steam", Some(period_s), None),
        CodeKind::TimeBased { period_s } => ("totp", Some(period_s), None),
        CodeKind::CounterBased => ("hotp", None, Some(candidate.counter)),
    };
    ImportRow {
        group,
        index,
        title: candidate.title.clone(),
        issuer: candidate.issuer.clone(),
        account: candidate.account.clone(),
        kind,
        algorithm: algorithm_name(spec),
        digits: spec.digits(),
        period_s,
        counter,
        sample_code: sample_code(candidate, now_ms),
        title_taken: false,
        lost: Vec::new(),
        existing_title: None,
        same_as_row: None,
    }
}

fn sample_code(candidate: &Candidate, now_ms: u64) -> Option<String> {
    if candidate.spec.kind() == CodeKind::CounterBased {
        return None;
    }
    let seconds = i64::try_from(now_ms / MS_PER_S).unwrap_or(i64::MAX);
    derive_passcode(&candidate.secret, candidate.spec, Moment::Time(seconds))
        .ok()
        .map(|p| p.code().to_owned())
}

const fn algorithm_name(spec: CodeSpec) -> &'static str {
    match spec.digest() {
        Digest::Sha1 => "SHA1",
        Digest::Sha256 => "SHA256",
        Digest::Sha512 => "SHA512",
        Digest::Md5 => "MD5",
    }
}

const fn lost_code(lost: Lost) -> &'static str {
    match lost {
        Lost::TitleShortened => "TITLE_SHORTENED",
        Lost::IssuerShortened => "ISSUER_SHORTENED",
        Lost::AccountShortened => "ACCOUNT_SHORTENED",
        Lost::NoteShortened => "NOTE_SHORTENED",
        Lost::TagsShortened => "TAGS_SHORTENED",
        Lost::TagsDropped => "TAGS_DROPPED",
    }
}

fn skipped_view(skipped: &Skipped) -> SkippedView {
    let place = match &skipped.place {
        Place::Line(line) => PlaceView::Line { line: *line },
        Place::TransferAccount { code, index } => PlaceView::Transfer {
            code: *code,
            index: *index,
        },
        Place::Named(name) => PlaceView::Named { name: name.clone() },
    };
    let (reason, detail) = match &skipped.reason {
        SkipReason::NotALink => ("NOT_A_LINK", None),
        SkipReason::Link(fault) => link_reason(fault),
        SkipReason::Transfer(_) => ("TRANSFER_UNREADABLE", None),
        SkipReason::TransferAccount(unsupported) => (
            "TRANSFER_ACCOUNT_UNSUPPORTED",
            Some(format!("{unsupported:?}")),
        ),
    };
    SkippedView {
        place,
        reason,
        detail,
    }
}

/// The code and detail of a link fault. Details name parameters and single characters, never the secret.
pub fn link_reason(fault: &KeyUriFault) -> (&'static str, Option<String>) {
    match fault {
        KeyUriFault::NotAUri => ("NOT_A_LINK", None),
        KeyUriFault::WrongScheme { found } => ("WRONG_SCHEME", Some(found.clone())),
        KeyUriFault::UnsupportedKind { found } => ("UNSUPPORTED_KIND", Some(found.clone())),
        KeyUriFault::SecretMissing => ("SECRET_MISSING", None),
        KeyUriFault::SecretInvalid { at, found } => {
            ("SECRET_INVALID", Some(format!("{}:{found}", at + 1)))
        }
        KeyUriFault::UnsupportedAlgorithm { found } => {
            ("UNSUPPORTED_ALGORITHM", Some(found.clone()))
        }
        KeyUriFault::ParamOutOfRange { name, value } => {
            ("PARAM_OUT_OF_RANGE", Some(format!("{name}={value}")))
        }
        KeyUriFault::GoogleExport => ("GOOGLE_EXPORT", None),
        KeyUriFault::MicrosoftPush => ("MICROSOFT_PUSH", None),
        KeyUriFault::OracleEnrolment => ("ORACLE_ENROLMENT", None),
        KeyUriFault::BareSecret => ("BARE_SECRET", None),
    }
}

//! What the page can ask for, and what the app behind the page provides.
//!
//! Requests and replies carry display fields only: never secrets, and no codes (design/07-platform.md §2.5).
//! Copying happens inside the app, so a code never travels to the page.

use serde::{Deserialize, Serialize};

/// One entry as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateEntry {
    /// The entry's id.
    pub id: String,
    /// Display name.
    pub title: String,
    /// Service name.
    pub issuer: String,
    /// Account name.
    pub account: String,
    /// Note.
    pub note: String,
    /// Shown first.
    pub pinned: bool,
    /// `totp` or `hotp`.
    pub kind: String,
    /// The entry's own global shortcut, in canonical form (for example `control+alt+super+KeyG`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// The default shortcut its place in the list gives it, when it has no own one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_shortcut: Option<String>,
}

/// A change to an entry's names; absent fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryEdit {
    /// New display name.
    pub title: Option<String>,
    /// New note.
    pub note: Option<String>,
}

/// A new shortcut for an entry, or `None` to remove it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShortcutChange {
    /// As the page recorded it, for example `Control+Alt+Super+KeyG`.
    pub shortcut: Option<String>,
}

/// What the page learns after a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Copied {
    /// Seconds until the clipboard is cleared, or 0 when it is not cleared.
    pub clears_in_s: u64,
}

/// Why the app refused a request. The page shows a message for each code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GateError {
    /// No such entry.
    #[error("the entry does not exist")]
    NotFound,
    /// A field is not acceptable (`TITLE_EMPTY`, `TITLE_TOO_LONG`, `NOTE_TOO_LONG`).
    #[error("the value is not acceptable: {0}")]
    Invalid(&'static str),
    /// Not a shortcut that can be used: no modifier besides Shift, or an unknown key.
    #[error("this key combination cannot be used as a shortcut")]
    ShortcutInvalid,
    /// Another entry or OTPKeep itself already uses it.
    #[error("this shortcut is already used by {0}")]
    ShortcutTaken(String),
    /// The system refused to register it; another app probably holds it.
    #[error("the system did not accept this shortcut; another app may be using it")]
    ShortcutRejected,
    /// The data is protected by a master password that has not been entered yet.
    #[error("enter the master password in OTPKeep first")]
    NeedsPassword,
    /// A newer OTPKeep uses the data.
    #[error("the data is read-only")]
    ReadOnly,
    /// The change could not be saved.
    #[error("the change could not be saved")]
    Save,
    /// An HOTP entry has no code until one is generated in OTPKeep.
    #[error("generate a code in OTPKeep first")]
    NoCodeYet,
    /// The clipboard could not be written.
    #[error("the code could not be copied")]
    Clipboard,
}

impl GateError {
    /// The code the page recognises.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::Invalid(code) => code,
            Self::ShortcutInvalid => "SHORTCUT_INVALID",
            Self::ShortcutTaken(_) => "SHORTCUT_TAKEN",
            Self::ShortcutRejected => "SHORTCUT_REJECTED",
            Self::NeedsPassword => "NEEDS_PASSWORD",
            Self::ReadOnly => "READ_ONLY",
            Self::Save => "SAVE_FAILED",
            Self::NoCodeYet => "NO_CODE_YET",
            Self::Clipboard => "COPY_FAILED",
        }
    }
}

/// A file of the page itself.
#[derive(Debug, Clone)]
pub struct Asset {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// Its media type.
    pub mime: String,
}

/// The app behind the page.
pub trait Backend: Send + Sync + 'static {
    /// Entries matching `query` (all of them when empty), in list order or best match first.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::NeedsPassword`] before the master password is entered.
    fn entries(&self, query: &str) -> Result<Vec<GateEntry>, GateError>;
    /// Changes an entry's name or note and returns it as it now is.
    ///
    /// # Errors
    ///
    /// Returns [`GateError`] when the change is refused; nothing changes then.
    fn edit(&self, id: &str, edit: EntryEdit) -> Result<GateEntry, GateError>;
    /// Sets or removes an entry's own shortcut and returns the entry as it now is.
    ///
    /// # Errors
    ///
    /// Returns [`GateError`] when the shortcut is refused; the old one stays then.
    fn set_shortcut(&self, id: &str, shortcut: Option<&str>) -> Result<GateEntry, GateError>;
    /// Copies the entry's current code to this computer's clipboard; the code is not returned.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::NoCodeYet`] for an HOTP entry without a code, [`GateError::Clipboard`] when
    /// writing failed, or [`GateError::NeedsPassword`] / [`GateError::NotFound`].
    fn copy(&self, id: &str) -> Result<Copied, GateError>;
    /// A file of the page (`gate.html`, `assets/…`).
    fn asset(&self, path: &str) -> Option<Asset>;
}

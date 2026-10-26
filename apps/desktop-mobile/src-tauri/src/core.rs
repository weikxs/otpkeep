//! What the quick panel shows and does, kept apart from Tauri so it can be tested (design/12 §6).
//!
//! The panel only ever receives display fields and codes; secrets stay here.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeKind, Passcode};
use keep_platform::Keystore;
use keep_vault::edit::{Change, EditError, NOTE_MAX_CHARS, TITLE_MAX_CHARS};
use keep_vault::open::{OpenVault, Startup, StopReason, UnlockError, start, unlock_with_password};
use keep_vault::service::{ServiceError, VaultService};
use keep_vault::store::VaultPaths;
use keep_vault::{Entry, EntryId};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::staging::Staging;

mod batch;
mod export;
mod import;
pub mod import_view;
mod key;
mod manage;
mod reopen;
mod search;
#[cfg(desktop)]
pub mod sync;
mod update;
mod upkeep;

pub use batch::BatchAction;
pub use export::{ExportFormat, GoogleExport, LeftOut, QrImage};
pub use import::{ImportProblem, ManualForm, read_bytes};
pub use import_view::ImportView;
pub use key::{KeyForm, KeyInfo};
pub use manage::{History, MoveTo, TrashItem};
pub use update::EntryUpdate;

use search::{FindCache, select};

/// The panel lists at most this many entries without a search (design/12 §3).
pub const PANEL_LIMIT: usize = 20;
/// Entries found by a search, at most.
pub const SEARCH_LIMIT: usize = 50;
const MS_PER_S: u64 = 1000;

/// The clock, in Unix milliseconds.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Each entry's place in the whole list by id, which decides its default shortcut.
pub type Places = HashMap<String, usize>;

/// One row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelEntry {
    pub id: String,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub note: String,
    pub tags: Vec<String>,
    pub kind: EntryKind,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_s: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until_ms: Option<u64>,
    /// Its global shortcut in canonical form; filled in by the shortcut layer, not here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// The entry's own shortcut, when it has one (as opposed to its row's default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own_shortcut: Option<String>,
}

/// One entry's descriptive fields, for the browser management page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub id: String,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub note: String,
    pub pinned: bool,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Totp,
    Hotp,
    /// Steam Guard: time-based, five characters.
    Steam,
}

/// There is no app lock (ADR-019): a vault protected by a master password asks for it once per run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PanelStatus {
    Ready,
    NeedsPassword,
    Unavailable,
}

/// Why the data cannot be opened, for the panel to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Problem {
    DataMissing,
    KeyMissing,
    Damaged,
    NewerFormat,
    KeystoreUnavailable,
    Busy,
    Io,
    NoDataDir,
}

/// Everything the panel shows.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts the windows read as they are, not a state machine"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelState {
    pub status: PanelStatus,
    pub entries: Vec<PanelEntry>,
    pub now_ms: u64,
    pub uses_master_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<Problem>,
    pub in_group: bool,
    pub read_only: bool,
    /// The vault file is damaged and its previous version can be put back.
    pub can_restore: bool,
}

/// Why an action on one entry failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("the master password has not been entered yet")]
    NeedsPassword,
    #[error("no such entry")]
    NotFound,
    #[error("no code has been generated for this entry yet")]
    NoCodeYet,
    #[error("the code cannot be computed")]
    Passcode,
    #[error("the data is read-only")]
    ReadOnly,
    #[error("the data could not be saved")]
    Save,
    /// A field is not acceptable; the code says which rule (`TITLE_EMPTY`, `TITLE_TOO_LONG`, `NOTE_TOO_LONG`).
    #[error("the value is not acceptable: {0}")]
    Invalid(&'static str),
    /// The typed secret has a character that is not Base32 at this position (from 0).
    #[error("the secret has '{found}' at position {at}")]
    SecretInvalid { at: usize, found: char },
    /// An import could not be staged or committed.
    #[error("the import could not go on: {0:?}")]
    Import(ImportProblem),
}

impl From<ServiceError> for CoreError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound | ServiceError::Edit(EditError::NotFound) => Self::NotFound,
            ServiceError::NoCodeYet => Self::NoCodeYet,
            ServiceError::Passcode(_) => Self::Passcode,
            ServiceError::Save(keep_vault::open::SaveError::ReadOnly) => Self::ReadOnly,
            ServiceError::Edit(EditError::TitleEmpty) => Self::Invalid("TITLE_EMPTY"),
            ServiceError::Edit(EditError::TitleTooLong) => Self::Invalid("TITLE_TOO_LONG"),
            ServiceError::Edit(EditError::NoteTooLong) => Self::Invalid("NOTE_TOO_LONG"),
            ServiceError::Edit(EditError::TagTooLong) => Self::Invalid("TAG_TOO_LONG"),
            ServiceError::Edit(EditError::IssuerTooLong) => Self::Invalid("ISSUER_TOO_LONG"),
            ServiceError::Edit(EditError::AccountTooLong) => Self::Invalid("ACCOUNT_TOO_LONG"),
            ServiceError::Edit(EditError::TooManyTags) => Self::Invalid("TOO_MANY_TAGS"),
            _ => Self::Save,
        }
    }
}

enum Vault {
    /// No data yet: the main window creates it on the first add or import.
    Fresh,
    Open {
        service: Box<VaultService>,
        read_only: bool,
        /// Kept with the vault it indexes: a vault opened anew starts without one.
        find: FindCache,
    },
    /// Protected by a master password that has not been entered in this run.
    NeedsPassword,
    Unavailable(Problem),
}

/// The app's view of the vault.
pub struct Core {
    paths: Option<VaultPaths>,
    keystore: Box<dyn Keystore>,
    clock: Clock,
    lock_wait: Duration,
    uses_master_password: bool,
    vault: Vault,
    /// The vault could not be opened and has a previous version to put back.
    restorable: bool,
    /// When the trash was last emptied of old entries.
    tidied_ms: Option<u64>,
    staging: Staging,
    /// The clock of the data the latest sync session started from; the session's result is merged with the
    /// data once more when it changed since.
    #[cfg(desktop)]
    session_base: Option<keep_vault::Hlc>,
}

impl Core {
    /// Looks at the data directory and opens the vault when no password is needed.
    #[must_use]
    pub fn start(
        paths: Option<VaultPaths>,
        keystore: Box<dyn Keystore>,
        clock: Clock,
        lock_wait: Duration,
    ) -> Self {
        let mut core = Self {
            paths,
            keystore,
            clock,
            lock_wait,
            uses_master_password: false,
            vault: Vault::NeedsPassword,
            restorable: false,
            tidied_ms: None,
            staging: Staging::default(),
            #[cfg(desktop)]
            session_base: None,
        };
        core.open();
        core
    }

    fn open(&mut self) {
        self.restorable = false;
        let Some(paths) = &self.paths else {
            self.vault = Vault::Unavailable(Problem::NoDataDir);
            return;
        };
        self.vault = match start(paths, self.keystore.as_ref(), self.lock_wait) {
            Startup::Fresh { .. } => Vault::Fresh,
            Startup::Ready(open) => self.opened(open, false),
            Startup::ReadOnly { open, .. } => self.opened(open, true),
            Startup::NeedsMasterPassword(_) => {
                self.uses_master_password = true;
                Vault::NeedsPassword
            }
            Startup::Stopped(reason) => self.stopped(reason),
        };
        self.tidy();
    }

    /// Why the vault cannot be opened; remembers whether a previous version can be put back.
    fn stopped(&mut self, reason: StopReason) -> Vault {
        self.restorable = matches!(
            reason,
            StopReason::Damaged {
                previous_available: true
            }
        );
        Vault::Unavailable(problem(reason))
    }

    fn opened(&self, open: keep_vault::open::OpenVault, read_only: bool) -> Vault {
        let clock = Arc::clone(&self.clock);
        Vault::Open {
            service: Box::new(VaultService::new(open, move || clock())),
            read_only,
            find: FindCache::default(),
        }
    }

    /// What the panel shows for `query`.
    #[must_use]
    pub fn state(&self, query: &str) -> PanelState {
        self.state_and_places(query, false).0
    }

    /// Every entry matching `query` (all of them when empty), as the main window shows them.
    #[cfg(test)]
    #[must_use]
    pub fn state_all(&self, query: &str) -> PanelState {
        self.state_and_places(query, true).0
    }

    /// What the panel (or with `everything`, the main window) shows for `query`, and every entry's place in
    /// the whole list, both from one pass over the list.
    #[must_use]
    pub fn state_and_places(&self, query: &str, everything: bool) -> (PanelState, Places) {
        let limit = (!everything).then_some(PANEL_LIMIT);
        let now_ms = (self.clock)();
        let mut places = Places::new();
        let (status, entries, problem, in_group, read_only) = match &self.vault {
            Vault::Fresh => (PanelStatus::Ready, Vec::new(), None, false, false),
            Vault::Open {
                service,
                read_only,
                find,
            } => {
                let ordered = service.ordered();
                places = ordered
                    .iter()
                    .enumerate()
                    .map(|(place, e)| (e.id.to_string(), place))
                    .collect();
                let rows = select(service, find, &ordered, query, limit)
                    .into_iter()
                    .map(|entry| row(service, entry, now_ms))
                    .collect();
                (
                    PanelStatus::Ready,
                    rows,
                    None,
                    service.doc().group.is_some(),
                    *read_only,
                )
            }
            Vault::NeedsPassword => (PanelStatus::NeedsPassword, Vec::new(), None, false, false),
            Vault::Unavailable(problem) => (
                PanelStatus::Unavailable,
                Vec::new(),
                Some(*problem),
                false,
                false,
            ),
        };
        let state = PanelState {
            status,
            entries,
            now_ms,
            uses_master_password: self.uses_master_password,
            problem,
            in_group,
            read_only,
            can_restore: self.restorable && problem == Some(Problem::Damaged),
        };
        (state, places)
    }

    /// The current code of `id`, for the clipboard.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the master password is still needed, the entry is gone or no code can be given.
    pub fn code(&self, id: &str) -> Result<Zeroizing<String>, CoreError> {
        let service = self.service()?;
        let entry = find(service, id)?;
        let passcode = service.passcode(entry)?;
        Ok(Zeroizing::new(passcode.code().to_owned()))
    }

    /// Generates the next HOTP code of `id` and returns the updated row.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn next_hotp(&mut self, id: &str) -> Result<PanelEntry, CoreError> {
        let now_ms = (self.clock)();
        let service = self.writable()?;
        let entry = find(service, id)?;
        service.next_hotp(entry)?;
        let entry = service.entry(entry).ok_or(CoreError::NotFound)?;
        Ok(row(service, entry, now_ms))
    }

    /// Every entry matching `query` (all of them, in list order, when empty), with names and notes.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NeedsPassword`] before the master password is entered.
    pub fn list(&self, query: &str) -> Result<Vec<EntryInfo>, CoreError> {
        let Vault::Open { service, find, .. } = &self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        let ordered = service.ordered();
        Ok(select(service, find, &ordered, query, None)
            .into_iter()
            .map(info)
            .collect())
    }

    /// Changes an entry's display name and / or note. Both are checked before either is saved.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn edit(
        &mut self,
        id: &str,
        title: Option<String>,
        note: Option<String>,
    ) -> Result<EntryInfo, CoreError> {
        check_title_and_note(title.as_deref(), note.as_deref())?;
        let service = self.writable()?;
        let entry = find(service, id)?;
        let changes = [title.map(Change::Title), note.map(Change::Note)]
            .into_iter()
            .flatten()
            .collect();
        manage::settle(service.edit_all(entry, changes))?;
        service.entry(entry).map(info).ok_or(CoreError::NotFound)
    }

    /// Opens a vault protected by a master password; it then stays open for this run. Returns the new state.
    /// The app does the same in steps (see [`Core::unlock_target`]).
    #[cfg(test)]
    pub fn unlock(&mut self, password: &str) -> PanelState {
        let opened = self.unlock_target().map(|target| target.open(password));
        self.finish_unlock(opened)
    }

    /// What unlocking needs while the vault waits for its master password. Deriving the key is the slow part
    /// and needs only this, so the app runs it without holding the core: the windows and sync keep working.
    #[must_use]
    pub fn unlock_target(&self) -> Option<UnlockTarget> {
        let (Vault::NeedsPassword, Some(paths)) = (&self.vault, &self.paths) else {
            return None;
        };
        Some(UnlockTarget {
            paths: paths.clone(),
            lock_wait: self.lock_wait,
        })
    }

    /// Takes the vault [`UnlockTarget::open`] opened (or why it did not) and returns the new state. A vault
    /// opened meanwhile by another unlock stays as it is.
    pub fn finish_unlock(&mut self, opened: Option<Result<OpenVault, UnlockError>>) -> PanelState {
        if matches!(self.vault, Vault::NeedsPassword) {
            match opened {
                Some(Ok(open)) => {
                    self.vault = self.opened(open, false);
                    self.tidy();
                }
                Some(Err(UnlockError::Stopped(reason))) => {
                    self.vault = self.stopped(reason);
                }
                Some(Err(UnlockError::WrongPassword)) | None => {}
            }
        }
        self.state("")
    }

    /// How many changes the open vault has saved in this run, to count writes in tests.
    #[cfg(test)]
    fn revision(&self) -> Option<u64> {
        self.service().ok().map(VaultService::revision)
    }

    fn service(&self) -> Result<&VaultService, CoreError> {
        match &self.vault {
            Vault::Open { service, .. } => Ok(service),
            _ => Err(CoreError::NeedsPassword),
        }
    }
}

/// Where a vault waiting for its master password is; see [`Core::unlock_target`].
pub struct UnlockTarget {
    paths: VaultPaths,
    lock_wait: Duration,
}

impl UnlockTarget {
    /// Derives the key from `password` (Argon2id with the vault's own parameters) and opens the vault.
    ///
    /// # Errors
    ///
    /// Returns [`UnlockError`] for a wrong password or a vault that cannot be opened.
    pub fn open(&self, password: &str) -> Result<OpenVault, UnlockError> {
        unlock_with_password(&self.paths, password, self.lock_wait)
    }
}

/// An entry in the list (not in the trash).
fn find(service: &VaultService, id: &str) -> Result<EntryId, CoreError> {
    find_any(service, id).and_then(|found| {
        service
            .entry(found)
            .filter(|e| e.trashed_at.value.is_none())
            .map(|e| e.id)
            .ok_or(CoreError::NotFound)
    })
}

/// Any entry, also one in the trash.
fn find_any(service: &VaultService, id: &str) -> Result<EntryId, CoreError> {
    EntryId::parse(id)
        .and_then(|id| service.entry(id))
        .map(|e| e.id)
        .ok_or(CoreError::NotFound)
}

fn kind_of(entry: &Entry) -> EntryKind {
    match entry.key.value.spec.kind() {
        CodeKind::TimeBased { .. } if entry.key.value.spec.is_steam() => EntryKind::Steam,
        CodeKind::TimeBased { .. } => EntryKind::Totp,
        CodeKind::CounterBased => EntryKind::Hotp,
    }
}

fn info(entry: &Entry) -> EntryInfo {
    EntryInfo {
        id: entry.id.to_string(),
        title: entry.title.value.clone(),
        issuer: entry.issuer.value.clone(),
        account: entry.account.value.clone(),
        note: entry.note.value.clone(),
        pinned: entry.pinned.value,
        kind: kind_of(entry),
    }
}

fn check_title_and_note(title: Option<&str>, note: Option<&str>) -> Result<(), CoreError> {
    if let Some(title) = title {
        if title.trim().is_empty() {
            return Err(CoreError::Invalid("TITLE_EMPTY"));
        }
        if title.chars().count() > TITLE_MAX_CHARS {
            return Err(CoreError::Invalid("TITLE_TOO_LONG"));
        }
    }
    if note.is_some_and(|n| n.chars().count() > NOTE_MAX_CHARS) {
        return Err(CoreError::Invalid("NOTE_TOO_LONG"));
    }
    Ok(())
}

fn row(service: &VaultService, entry: &Entry, now_ms: u64) -> PanelEntry {
    let passcode: Option<Passcode> = service.passcode_of(entry).ok();
    let kind = kind_of(entry);
    let period_s = match entry.key.value.spec.kind() {
        CodeKind::TimeBased { period_s } => Some(period_s),
        CodeKind::CounterBased => None,
    };
    let valid_until_ms = match (period_s, &passcode) {
        (Some(period), Some(_)) => {
            let step_ms = u64::from(period) * MS_PER_S;
            Some((now_ms / step_ms + 1) * step_ms)
        }
        _ => None,
    };
    PanelEntry {
        id: entry.id.to_string(),
        title: entry.title.value.clone(),
        issuer: entry.issuer.value.clone(),
        account: entry.account.value.clone(),
        note: entry.note.value.clone(),
        tags: entry.tags.value.clone(),
        kind,
        pinned: entry.pinned.value,
        code: passcode.as_ref().map(|p| p.code().to_owned()),
        next_code: passcode.as_ref().and_then(|p| p.next().map(str::to_owned)),
        period_s,
        valid_until_ms,
        shortcut: None,
        own_shortcut: None,
    }
}

const fn problem(reason: StopReason) -> Problem {
    match reason {
        StopReason::DataMissing => Problem::DataMissing,
        StopReason::KeyMissing => Problem::KeyMissing,
        StopReason::Damaged { .. } => Problem::Damaged,
        StopReason::NewerFormat(_) => Problem::NewerFormat,
        StopReason::KeystoreUnavailable => Problem::KeystoreUnavailable,
        StopReason::Busy => Problem::Busy,
        StopReason::Io => Problem::Io,
    }
}

#[cfg(test)]
mod tests;

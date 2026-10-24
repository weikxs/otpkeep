//! What the quick panel shows and does, kept apart from Tauri so it can be tested (design/12 §6).
//!
//! The panel only ever receives display fields and codes; secrets stay here.

use std::sync::Arc;
use std::time::Duration;

use keep_find::{FindContext, FindIndex};
use keep_passcode::{CodeKind, Passcode};
use keep_platform::Keystore;
use keep_vault::edit::{Change, EditError, NOTE_MAX_CHARS, TITLE_MAX_CHARS};
use keep_vault::open::{Startup, StopReason, UnlockError, start, unlock_with_password};
use keep_vault::service::{ServiceError, VaultService};
use keep_vault::store::VaultPaths;
use keep_vault::{Entry, EntryId};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::staging::Staging;

mod import;
mod import_view;

pub use import::{ImportProblem, ManualForm};
pub use import_view::ImportView;

/// The panel lists at most this many entries without a search (design/12 §3).
pub const PANEL_LIMIT: usize = 20;
/// Entries found by a search, at most.
pub const SEARCH_LIMIT: usize = 50;
const MS_PER_S: u64 = 1000;

/// The clock, in Unix milliseconds.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// One row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelEntry {
    pub id: String,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub note: String,
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
    staging: Staging,
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
            staging: Staging::default(),
        };
        core.open();
        core
    }

    fn open(&mut self) {
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
            Startup::Stopped(reason) => Vault::Unavailable(problem(reason)),
        };
    }

    fn opened(&self, open: keep_vault::open::OpenVault, read_only: bool) -> Vault {
        let clock = Arc::clone(&self.clock);
        Vault::Open {
            service: Box::new(VaultService::new(open, move || clock())),
            read_only,
        }
    }

    /// What the panel shows for `query`.
    #[must_use]
    pub fn state(&self, query: &str) -> PanelState {
        self.state_limited(query, Some(PANEL_LIMIT))
    }

    /// Every entry matching `query` (all of them when empty), for the main window.
    #[must_use]
    pub fn state_all(&self, query: &str) -> PanelState {
        self.state_limited(query, None)
    }

    fn state_limited(&self, query: &str, limit: Option<usize>) -> PanelState {
        let now_ms = (self.clock)();
        let (status, entries, problem, in_group, read_only) = match &self.vault {
            Vault::Fresh => (PanelStatus::Ready, Vec::new(), None, false, false),
            Vault::Open { service, read_only } => (
                PanelStatus::Ready,
                rows(service, query, limit, now_ms),
                None,
                service.doc().group.is_some(),
                *read_only,
            ),
            Vault::NeedsPassword => (PanelStatus::NeedsPassword, Vec::new(), None, false, false),
            Vault::Unavailable(problem) => (
                PanelStatus::Unavailable,
                Vec::new(),
                Some(*problem),
                false,
                false,
            ),
        };
        PanelState {
            status,
            entries,
            now_ms,
            uses_master_password: self.uses_master_password,
            problem,
            in_group,
            read_only,
        }
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
        let Vault::Open { service, read_only } = &mut self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        if *read_only {
            return Err(CoreError::ReadOnly);
        }
        let entry = find(service, id)?;
        service.next_hotp(entry)?;
        rows(service, "", None, now_ms)
            .into_iter()
            .find(|row| row.id == id)
            .ok_or(CoreError::NotFound)
    }

    /// Every entry matching `query` (all of them, in list order, when empty), with names and notes.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NeedsPassword`] before the master password is entered.
    pub fn list(&self, query: &str) -> Result<Vec<EntryInfo>, CoreError> {
        let service = self.service()?;
        Ok(select(service, query, None).into_iter().map(info).collect())
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
        let Vault::Open { service, read_only } = &mut self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        if *read_only {
            return Err(CoreError::ReadOnly);
        }
        let entry = find(service, id)?;
        for change in [title.map(Change::Title), note.map(Change::Note)]
            .into_iter()
            .flatten()
        {
            match service.edit(entry, change) {
                // The change itself is saved; only this device's history of it is missing.
                Ok(()) | Err(ServiceError::HistoryNotSaved) => {}
                Err(error) => return Err(error.into()),
            }
        }
        service.entry(entry).map(info).ok_or(CoreError::NotFound)
    }

    /// Opens a vault protected by a master password; it then stays open for this run. Returns the new state.
    pub fn unlock(&mut self, password: &str) -> PanelState {
        let (Vault::NeedsPassword, Some(paths)) = (&self.vault, &self.paths) else {
            return self.state("");
        };
        match unlock_with_password(paths, password, self.lock_wait) {
            Ok(open) => self.vault = self.opened(open, false),
            Err(UnlockError::WrongPassword) => {}
            Err(UnlockError::Stopped(reason)) => self.vault = Vault::Unavailable(problem(reason)),
        }
        self.state("")
    }

    fn service(&self) -> Result<&VaultService, CoreError> {
        match &self.vault {
            Vault::Open { service, .. } => Ok(service),
            _ => Err(CoreError::NeedsPassword),
        }
    }
}

fn find(service: &VaultService, id: &str) -> Result<EntryId, CoreError> {
    service
        .ordered()
        .iter()
        .find(|e| e.id.to_string() == id)
        .map(|e| e.id)
        .ok_or(CoreError::NotFound)
}

/// Without a search, the entries in list order (pinned first), at most `limit`; with one, the best matches.
fn select<'a>(service: &'a VaultService, query: &str, limit: Option<usize>) -> Vec<&'a Entry> {
    let ordered = service.ordered();
    if query.trim().is_empty() {
        return ordered
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .collect();
    }
    let index = FindIndex::build(&ordered);
    index
        .find(query, &FindContext::default())
        .into_iter()
        .take(SEARCH_LIMIT)
        .filter_map(|hit| ordered.iter().find(|e| e.id == hit.entry).copied())
        .collect()
}

/// The panel's rows: at most [`PANEL_LIMIT`] without a search.
fn rows(service: &VaultService, query: &str, limit: Option<usize>, now_ms: u64) -> Vec<PanelEntry> {
    select(service, query, limit)
        .into_iter()
        .map(|entry| row(service, entry, now_ms))
        .collect()
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
    let passcode: Option<Passcode> = service.passcode(entry.id).ok();
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
        kind,
        pinned: entry.pinned.value,
        code: passcode.as_ref().map(|p| p.code().to_owned()),
        next_code: passcode.as_ref().and_then(|p| p.next().map(str::to_owned)),
        period_s,
        valid_until_ms,
        shortcut: None,
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

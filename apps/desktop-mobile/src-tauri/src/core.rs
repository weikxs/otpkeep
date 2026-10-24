//! What the quick panel shows and does, kept apart from Tauri so it can be tested (design/12 §6).
//!
//! The panel only ever receives display fields and codes; secrets stay here.

use std::sync::Arc;
use std::time::Duration;

use keep_find::{FindContext, FindIndex};
use keep_passcode::{CodeKind, Passcode};
use keep_platform::Keystore;
use keep_vault::open::{Startup, StopReason, UnlockError, start, unlock_with_password};
use keep_vault::service::{ServiceError, VaultService};
use keep_vault::store::VaultPaths;
use keep_vault::{Entry, EntryId};
use serde::Serialize;
use zeroize::Zeroizing;

/// The panel lists at most this many entries without a search (design/12 §3).
pub const PANEL_LIMIT: usize = 20;
/// Entries found by a search, at most.
pub const SEARCH_LIMIT: usize = 50;
/// The first rows get the global shortcuts ⌃⌥⌘1 … ⌃⌥⌘0.
const SHORTCUT_ROWS: usize = 10;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Totp,
    Hotp,
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
}

impl From<ServiceError> for CoreError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::NotFound => Self::NotFound,
            ServiceError::NoCodeYet => Self::NoCodeYet,
            ServiceError::Passcode(_) => Self::Passcode,
            ServiceError::Save(keep_vault::open::SaveError::ReadOnly) => Self::ReadOnly,
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
        let now_ms = (self.clock)();
        let (status, entries, problem, in_group, read_only) = match &self.vault {
            Vault::Fresh => (PanelStatus::Ready, Vec::new(), None, false, false),
            Vault::Open { service, read_only } => (
                PanelStatus::Ready,
                rows(service, query, now_ms),
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
        rows(service, "", now_ms)
            .into_iter()
            .find(|row| row.id == id)
            .ok_or(CoreError::NotFound)
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

/// The rows: without a search the first [`PANEL_LIMIT`] in list order (pinned first); with one, the best
/// matches.
fn rows(service: &VaultService, query: &str, now_ms: u64) -> Vec<PanelEntry> {
    let ordered = service.ordered();
    let chosen: Vec<&Entry> = if query.trim().is_empty() {
        ordered.into_iter().take(PANEL_LIMIT).collect()
    } else {
        let index = FindIndex::build(&ordered);
        index
            .find(query, &FindContext::default())
            .into_iter()
            .take(SEARCH_LIMIT)
            .filter_map(|hit| ordered.iter().find(|e| e.id == hit.entry).copied())
            .collect()
    };
    chosen
        .into_iter()
        .enumerate()
        .map(|(index, entry)| row(service, entry, index, now_ms))
        .collect()
}

fn row(service: &VaultService, entry: &Entry, index: usize, now_ms: u64) -> PanelEntry {
    let passcode: Option<Passcode> = service.passcode(entry.id).ok();
    let (kind, period_s) = match entry.key.value.spec.kind() {
        CodeKind::TimeBased { period_s } => (EntryKind::Totp, Some(period_s)),
        CodeKind::CounterBased => (EntryKind::Hotp, None),
    };
    let valid_until_ms = match (kind, period_s, &passcode) {
        (EntryKind::Totp, Some(period), Some(_)) => {
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
        kind,
        pinned: entry.pinned.value,
        code: passcode.as_ref().map(|p| p.code().to_owned()),
        next_code: passcode.as_ref().and_then(|p| p.next().map(str::to_owned)),
        period_s,
        valid_until_ms,
        shortcut: (index < SHORTCUT_ROWS).then(|| u8::try_from(index + 1).unwrap_or(u8::MAX)),
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

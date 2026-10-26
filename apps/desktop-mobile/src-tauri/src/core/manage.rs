//! Keeping the list in order: pinning, moving, tags, the trash and the history of names and notes
//! (PRD FR-3.1, FR-3.11, FR-4.1–4.3).

use keep_vault::edit::{Change, Placement, TRASH_RETENTION_MS};
use keep_vault::history::HistoryField;
use keep_vault::service::{ServiceError, VaultService};
use serde::{Deserialize, Serialize};

use super::{Core, CoreError, Vault, find_any};

/// Where to move an entry, as the window says it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "to")]
pub enum MoveTo {
    First,
    Last,
    Before { other: String },
    After { other: String },
}

/// An entry in the trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: String,
    pub title: String,
    pub issuer: String,
    pub account: String,
    pub trashed_at_ms: u64,
    /// When it will be deleted for good.
    pub purge_at_ms: u64,
}

/// One earlier version of a name or note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub value: String,
    pub at_ms: u64,
}

/// The earlier names and notes of an entry, newest first (kept on this device only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub title: Vec<Version>,
    pub note: Vec<Version>,
}

impl Core {
    /// Pins or unpins an entry.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        settle(service.edit(entry, Change::Pinned(pinned)))
    }

    /// Replaces an entry's tags; blank tags are dropped and repeats kept once.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] with `TAG_TOO_LONG` or `TOO_MANY_TAGS`.
    pub fn set_tags(&mut self, id: &str, tags: Vec<String>) -> Result<(), CoreError> {
        let mut clean: Vec<String> = Vec::with_capacity(tags.len());
        for tag in tags {
            let tag = tag.trim().to_owned();
            if !tag.is_empty() && !clean.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
                clean.push(tag);
            }
        }
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        settle(service.edit(entry, Change::Tags(clean)))
    }

    /// Moves an entry in the list; the order is the same on every device.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn move_to(&mut self, id: &str, to: &MoveTo) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        let placement = match to {
            MoveTo::First => Placement::First,
            MoveTo::Last => Placement::Last,
            MoveTo::Before { other } => Placement::Before(find_any(service, other)?),
            MoveTo::After { other } => Placement::After(find_any(service, other)?),
        };
        Ok(service.move_entry(entry, placement)?)
    }

    /// Moves an entry to the trash, where it stays 30 days.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn trash(&mut self, id: &str) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        Ok(service.trash(entry)?)
    }

    /// Takes an entry out of the trash.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn restore(&mut self, id: &str) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        Ok(service.restore(entry)?)
    }

    /// Deletes an entry from the trash for good.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn purge(&mut self, id: &str) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find_any(service, id)?;
        Ok(service.purge(entry)?)
    }

    /// The trash, most recently trashed first.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NeedsPassword`] before the master password.
    pub fn trash_list(&self) -> Result<Vec<TrashItem>, CoreError> {
        let Vault::Open { service, .. } = &self.vault else {
            return match self.vault {
                Vault::Fresh => Ok(Vec::new()),
                _ => Err(CoreError::NeedsPassword),
            };
        };
        Ok(service
            .trashed()
            .into_iter()
            .map(|e| {
                let trashed_at_ms = e.trashed_at.value.map_or(0, |at| at.wall_ms);
                TrashItem {
                    id: e.id.to_string(),
                    title: e.title.value.clone(),
                    issuer: e.issuer.value.clone(),
                    account: e.account.value.clone(),
                    trashed_at_ms,
                    purge_at_ms: trashed_at_ms.saturating_add(TRASH_RETENTION_MS),
                }
            })
            .collect())
    }

    /// The earlier names and notes of an entry.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the data is not open or the entry is gone.
    pub fn history(&self, id: &str) -> Result<History, CoreError> {
        let Vault::Open { service, .. } = &self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        let entry = find_any(service, id)?;
        let versions = |field| {
            service
                .history(entry, field)
                .into_iter()
                .map(|v| Version {
                    value: v.value.clone(),
                    at_ms: v.at_ms,
                })
                .collect()
        };
        Ok(History {
            title: versions(HistoryField::Title),
            note: versions(HistoryField::Note),
        })
    }

    /// Deletes what has been in the trash for more than 30 days; called whenever the data opens, and daily.
    pub(super) fn tidy(&mut self) {
        if matches!(self.vault, Vault::Open { .. }) {
            self.tidied_ms = Some((self.clock)());
        }
        if let Vault::Open {
            service,
            read_only: false,
            ..
        } = &mut self.vault
            && service.purge_expired_trash().is_err()
        {
            tracing::warn!("old entries in the trash could not be deleted yet");
        }
    }

    /// The service, when it may be changed.
    pub(super) fn writable(&mut self) -> Result<&mut VaultService, CoreError> {
        match &mut self.vault {
            Vault::Open {
                read_only: true, ..
            } => Err(CoreError::ReadOnly),
            Vault::Open { service, .. } => Ok(service),
            _ => Err(CoreError::NeedsPassword),
        }
    }
}

/// A change whose history could not be saved is still a change made.
pub(super) fn settle(result: Result<(), ServiceError>) -> Result<(), CoreError> {
    match result {
        Ok(()) | Err(ServiceError::HistoryNotSaved) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests;

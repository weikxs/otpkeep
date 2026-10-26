//! Saving the main window's editor (PRD FR-3.1): every field the person changed is checked and saved together,
//! so one Save writes the vault once and either all changes are kept or none.

use keep_vault::edit::Change;
use serde::Deserialize;

use super::manage::settle;
use super::{Core, CoreError, check_title_and_note, find};

/// The fields of one entry the editor changed; absent fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EntryUpdate {
    pub title: Option<String>,
    pub note: Option<String>,
    pub issuer: Option<String>,
    pub account: Option<String>,
    pub tags: Option<Vec<String>>,
    pub pinned: Option<bool>,
}

impl Core {
    /// Changes an entry's names, note, tags and pin in one save; the old name and note go into the history.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] (for example `TITLE_EMPTY` or `TAG_TOO_LONG`); nothing changes on failure.
    pub fn update(&mut self, id: &str, update: EntryUpdate) -> Result<(), CoreError> {
        check_title_and_note(update.title.as_deref(), update.note.as_deref())?;
        let service = self.writable()?;
        let entry = find(service, id)?;
        let changes = [
            update.title.map(Change::Title),
            update.note.map(Change::Note),
            update.issuer.map(Change::Issuer),
            update.account.map(Change::Account),
            update.tags.map(Change::Tags),
            update.pinned.map(Change::Pinned),
        ]
        .into_iter()
        .flatten()
        .collect();
        settle(service.edit_all(entry, changes))
    }
}

#[cfg(test)]
mod tests;

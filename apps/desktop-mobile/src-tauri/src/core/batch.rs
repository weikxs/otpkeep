//! Changing several entries at once: to the trash, adding or removing a tag (PRD FR-4.5). The changes are
//! made on a copy of the data and saved together, so either all of them happen or none.

use keep_vault::edit::{Change, edit_entry, move_to_trash};
use keep_vault::service::ServiceError;
use serde::Deserialize;

use super::{Core, CoreError, find};

/// What to do with the chosen entries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum BatchAction {
    Trash,
    AddTag { tag: String },
    RemoveTag { tag: String },
}

impl Core {
    /// Applies `action` to the entries `ids` and returns how many changed; entries the action does not change
    /// (a tag already there, or not there) are left alone.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] (for example `TAG_TOO_LONG`, `TOO_MANY_TAGS` or `NotFound`); nothing changes then.
    pub fn batch(&mut self, ids: &[String], action: &BatchAction) -> Result<usize, CoreError> {
        let now = (self.clock)();
        let service = self.writable()?;
        let mut doc = service.doc().clone();
        let mut changed = 0;
        for id in ids {
            let entry_id = find(service, id)?;
            let current = doc
                .entries
                .iter()
                .find(|e| e.id == entry_id)
                .ok_or(CoreError::NotFound)?;
            let tags = &current.tags.value;
            let next = match action {
                BatchAction::Trash => Some(move_to_trash(&doc, entry_id, now)),
                BatchAction::AddTag { tag } => {
                    let tag = tag.trim();
                    if tag.is_empty() {
                        return Err(CoreError::Invalid("TAG_EMPTY"));
                    }
                    (!tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase())).then(|| {
                        let more = tags.iter().cloned().chain([tag.to_owned()]).collect();
                        edit_entry(&doc, entry_id, Change::Tags(more), now)
                    })
                }
                BatchAction::RemoveTag { tag } => tags.iter().any(|t| t == tag).then(|| {
                    let fewer = tags.iter().filter(|t| *t != tag).cloned().collect();
                    edit_entry(&doc, entry_id, Change::Tags(fewer), now)
                }),
            };
            if let Some(next) = next {
                doc = next.map_err(|error| CoreError::from(ServiceError::Edit(error)))?;
                changed += 1;
            }
        }
        if changed > 0 {
            service.commit_derived(doc)?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests;

//! Changing several entries at once: to the trash, adding or removing a tag (PRD FR-4.5). The changes are
//! made on a copy of the data and saved together, so either all of them happen or none.

use std::collections::HashMap;

use keep_vault::edit::{Change, edit_entry_mut, move_to_trash_mut};
use keep_vault::service::ServiceError;
use keep_vault::{Entry, EntryId};
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
        // One copy of the vault for the whole batch, changed in place and saved once.
        let mut doc = service.doc().clone();
        let positions: HashMap<EntryId, usize> = doc
            .entries
            .iter()
            .enumerate()
            .map(|(index, e)| (e.id, index))
            .collect();
        let mut changed = 0;
        for id in ids {
            // Chosen entries must be in the list as it was before the batch.
            let entry_id = find(service, id)?;
            let current = positions
                .get(&entry_id)
                .and_then(|&index| doc.entries.get(index))
                .ok_or(CoreError::NotFound)?;
            let Some(step) = step_for(current, action)? else {
                continue;
            };
            let applied = match step {
                Step::Edit(change) => edit_entry_mut(&mut doc, entry_id, change, now),
                Step::Trash => move_to_trash_mut(&mut doc, entry_id, now),
            };
            applied.map_err(|error| CoreError::from(ServiceError::Edit(error)))?;
            changed += 1;
        }
        if changed > 0 {
            service.commit_derived(doc)?;
        }
        Ok(changed)
    }
}

/// What a batch action does to one entry.
enum Step {
    Edit(Change),
    Trash,
}

/// What `action` does to `entry`; `None` when it leaves the entry as it is.
fn step_for(entry: &Entry, action: &BatchAction) -> Result<Option<Step>, CoreError> {
    let tags = &entry.tags.value;
    Ok(match action {
        BatchAction::Trash => Some(Step::Trash),
        BatchAction::AddTag { tag } => {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err(CoreError::Invalid("TAG_EMPTY"));
            }
            (!tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase())).then(|| {
                let more = tags.iter().cloned().chain([tag.to_owned()]).collect();
                Step::Edit(Change::Tags(more))
            })
        }
        BatchAction::RemoveTag { tag } => tags.iter().any(|t| t == tag).then(|| {
            let fewer = tags.iter().filter(|t| *t != tag).cloned().collect();
            Step::Edit(Change::Tags(fewer))
        }),
    })
}

#[cfg(test)]
mod tests;

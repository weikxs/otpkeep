//! Work after a merge: removing duplicate keys and clearing old tombstones (design/06-sync.md §6.4–6.5).

use std::collections::{BTreeMap, BTreeSet};

use keep_vault::edit::move_to_trash;
use keep_vault::{DeviceId, Entry, EntryId, VaultDoc};

/// Tombstones are kept at least this long, even when every member holds them.
pub const TOMBSTONE_GRACE_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Moves to the trash every entry outside the trash whose secret another such entry has, unless either was
/// added with "keep duplicate". The entry with the earliest `(created_at, id)` stays, so every device makes
/// the same choice. Names, notes and tags are left as they are; the user can restore from the trash.
///
/// Returns the new state and the entries moved, which the user is told about.
#[must_use]
pub fn dedupe(doc: &VaultDoc, now_ms: u64) -> (VaultDoc, Vec<EntryId>) {
    let mut by_secret: BTreeMap<&[u8], Vec<&Entry>> = BTreeMap::new();
    for entry in doc
        .entries
        .iter()
        .filter(|e| e.trashed_at.value.is_none() && !e.keep_duplicate.value)
    {
        by_secret
            .entry(entry.key.value.secret.expose())
            .or_default()
            .push(entry);
    }
    let mut extra: Vec<EntryId> = by_secret
        .into_values()
        .filter(|group| group.len() > 1)
        .flat_map(|mut group| {
            group.sort_by_key(|e| (e.created_at, e.id));
            group.into_iter().skip(1).map(|e| e.id).collect::<Vec<_>>()
        })
        .collect();
    extra.sort();
    let mut result = doc.clone();
    for id in &extra {
        // The ids come from `doc`, so the entry exists.
        if let Ok(next) = move_to_trash(&result, *id, now_ms) {
            result = next;
        }
    }
    if !extra.is_empty() {
        tracing::info!(count = extra.len(), "duplicate keys moved to the trash");
    }
    (result, extra)
}

/// Clears the tombstones that every device in `required` holds and that are older than
/// [`TOMBSTONE_GRACE_MS`]: none of them can bring their entries back any more. Pass
/// [`Membership::required_for_cleanup`](crate::membership::Membership::required_for_cleanup).
#[must_use]
pub fn prune_tombstones(doc: &VaultDoc, required: &BTreeSet<DeviceId>, now_ms: u64) -> VaultDoc {
    let tombstones = doc
        .tombstones
        .iter()
        .filter(|t| {
            let old = now_ms.saturating_sub(t.stamp.wall_ms) > TOMBSTONE_GRACE_MS;
            let everyone = required.iter().all(|m| t.acked_by.contains(&m.0));
            !(old && everyone)
        })
        .cloned()
        .collect();
    VaultDoc {
        tombstones,
        ..doc.clone()
    }
}

#[cfg(test)]
mod tests;

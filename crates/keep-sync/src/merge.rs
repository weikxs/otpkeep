//! Merging two devices' states (design/06-sync.md §6.1–6.3, §6.5).
//!
//! There is no change log: the states themselves are compared, field by field, and every rule gives the same
//! result whichever side runs it and in whatever order devices meet, so all devices converge.
//!
//! - Ordinary fields: the later timestamp wins (ties, which only a broken device could cause, go to the larger
//!   value, so both sides still agree).
//! - Key material moves as a whole. The same key on both sides keeps the higher HOTP counter; different keys
//!   keep the newer key, and the other becomes a copy in the trash whose id depends only on the entry and that
//!   key, so every device makes the same copy and no key is ever silently lost.
//! - Tombstones merge as a set. One with an entry changed after it is dropped, and the entry goes to the trash
//!   at the deletion time instead; otherwise the entry is deleted.
//! - Fields from newer versions are kept, taken from the side that changed the entry last.

use std::collections::BTreeMap;

use keep_vault::edit::old_key_copy;
use keep_vault::{
    DeviceId, Entry, EntryId, Extra, Field, Hlc, KeyMaterial, SyncGroup, Tombstone, VaultDoc,
};

use crate::membership::{merge_events, merge_presence};

/// Our state merged with theirs. Only the synced parts change: entries, tombstones and the group's events
/// and presence. `participants` are the two devices in the session; both are recorded as holding every
/// tombstone.
#[must_use]
pub fn merge(ours: &VaultDoc, theirs: &VaultDoc, participants: &[DeviceId]) -> VaultDoc {
    let mut entries: BTreeMap<EntryId, Entry> = BTreeMap::new();
    for entry in ours.entries.iter().chain(&theirs.entries) {
        upsert(&mut entries, entry.clone());
    }
    let mut tombstones = merge_tombstones(&ours.tombstones, &theirs.tombstones, participants);
    settle_tombstones(&mut entries, &mut tombstones);
    let entries: Vec<Entry> = entries.into_values().collect();
    let tombstones: Vec<Tombstone> = tombstones.into_values().collect();
    let clock = latest_stamp(&entries, &tombstones)
        .into_iter()
        .chain([ours.clock, theirs.clock])
        .max()
        .unwrap_or(ours.clock);
    VaultDoc {
        entries,
        tombstones,
        group: merge_group(ours.group.as_ref(), theirs.group.as_ref()),
        clock,
        ..ours.clone()
    }
}

/// Adds `entry`, merging it into an entry with the same id; copies of losing keys are added the same way.
fn upsert(entries: &mut BTreeMap<EntryId, Entry>, entry: Entry) {
    let mut pending = vec![entry];
    while let Some(next) = pending.pop() {
        let merged = match entries.remove(&next.id) {
            None => next,
            Some(current) => {
                let (merged, copy) = merge_entry(&current, &next);
                pending.extend(copy);
                merged
            }
        };
        entries.insert(merged.id, merged);
    }
}

/// One entry from two sides, and the trashed copy of a key that lost.
fn merge_entry(a: &Entry, b: &Entry) -> (Entry, Option<Entry>) {
    let (key, copy) = merge_key(a, b);
    let entry = Entry {
        id: a.id,
        created_at: a.created_at.min(b.created_at),
        title: later(&a.title, &b.title),
        issuer: later(&a.issuer, &b.issuer),
        account: later(&a.account, &b.account),
        key,
        note: later(&a.note, &b.note),
        tags: later(&a.tags, &b.tags),
        rank: later(&a.rank, &b.rank),
        pinned: later(&a.pinned, &b.pinned),
        keep_duplicate: later(&a.keep_duplicate, &b.keep_duplicate),
        trashed_at: later(&a.trashed_at, &b.trashed_at),
        old_key_of: later(&a.old_key_of, &b.old_key_of),
        extra: merge_extra(a, b),
    };
    (entry, copy)
}

/// The field with the later timestamp; on equal timestamps the larger value, so both sides agree.
fn later<T: Ord + Clone>(a: &Field<T>, b: &Field<T>) -> Field<T> {
    if (b.stamp, &b.value) > (a.stamp, &a.value) {
        b.clone()
    } else {
        a.clone()
    }
}

/// Key material from two sides (design/06-sync.md §6.3).
fn merge_key(a: &Entry, b: &Entry) -> (Field<KeyMaterial>, Option<Entry>) {
    let (ka, kb) = (&a.key.value, &b.key.value);
    if ka.key_id == kb.key_id {
        let rank =
            |k: &Field<KeyMaterial>| (k.value.counter, k.stamp, k.value.secret.expose().to_vec());
        let value = if rank(&b.key) > rank(&a.key) { kb } else { ka };
        let field = Field {
            value: value.clone(),
            stamp: a.key.stamp.max(b.key.stamp),
        };
        return (field, None);
    }
    let (winner, loser) = if kb.key_id > ka.key_id {
        (b, a)
    } else {
        (a, b)
    };
    let copy = old_key_copy(loser, winner.key.value.key_id);
    (winner.key.clone(), Some(copy))
}

/// Unknown fields: all keys from both sides; where both have a key, the side changed last wins.
fn merge_extra(a: &Entry, b: &Entry) -> Extra {
    let (first, second) =
        if (newest_field(b), encoded(&b.extra)) > (newest_field(a), encoded(&a.extra)) {
            (a, b)
        } else {
            (b, a)
        };
    let mut extra = first.extra.clone();
    extra.extend(second.extra.iter().map(|(k, v)| (*k, v.clone())));
    extra
}

fn encoded(extra: &Extra) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (key, value) in extra {
        bytes.extend_from_slice(&key.to_be_bytes());
        // Serializing an in-memory value into a Vec cannot fail.
        let _ = ciborium::ser::into_writer(value, &mut bytes);
    }
    bytes
}

/// The latest timestamp of any of the entry's fields.
pub(crate) fn newest_field(entry: &Entry) -> Hlc {
    [
        entry.created_at,
        entry.title.stamp,
        entry.issuer.stamp,
        entry.account.stamp,
        entry.key.stamp,
        entry.note.stamp,
        entry.tags.stamp,
        entry.rank.stamp,
        entry.pinned.stamp,
        entry.keep_duplicate.stamp,
        entry.trashed_at.stamp,
        entry.old_key_of.stamp,
    ]
    .into_iter()
    .max()
    .unwrap_or(entry.created_at)
}

fn merge_tombstones(
    ours: &[Tombstone],
    theirs: &[Tombstone],
    participants: &[DeviceId],
) -> BTreeMap<EntryId, Tombstone> {
    let mut merged: BTreeMap<EntryId, Tombstone> = BTreeMap::new();
    for tomb in ours.iter().chain(theirs) {
        merged
            .entry(tomb.entry)
            .and_modify(|current| {
                current.stamp = current.stamp.max(tomb.stamp);
                current.acked_by.extend(&tomb.acked_by);
            })
            .or_insert_with(|| tomb.clone());
    }
    for tomb in merged.values_mut() {
        tomb.acked_by.extend(participants.iter().map(|d| d.0));
        tomb.acked_by.sort_unstable();
        tomb.acked_by.dedup();
    }
    merged
}

/// Applies tombstones to entries that are still present (design/06-sync.md §6.2).
fn settle_tombstones(
    entries: &mut BTreeMap<EntryId, Entry>,
    tombstones: &mut BTreeMap<EntryId, Tombstone>,
) {
    tombstones.retain(|id, tomb| {
        let Some(entry) = entries.get_mut(id) else {
            return true;
        };
        if newest_field(entry) > tomb.stamp {
            entry.trashed_at = later(
                &entry.trashed_at,
                &Field {
                    value: Some(tomb.stamp),
                    stamp: tomb.stamp,
                },
            );
            return false;
        }
        entries.remove(id);
        true
    });
}

fn merge_group(ours: Option<&SyncGroup>, theirs: Option<&SyncGroup>) -> Option<SyncGroup> {
    let ours = ours?;
    let Some(theirs) = theirs.filter(|t| t.group_id == ours.group_id) else {
        return Some(ours.clone());
    };
    Some(SyncGroup {
        events: merge_events(&ours.group_id, &ours.events, &theirs.events),
        presence: merge_presence(&ours.presence, &theirs.presence),
        ..ours.clone()
    })
}

fn latest_stamp(entries: &[Entry], tombstones: &[Tombstone]) -> Option<Hlc> {
    entries
        .iter()
        .map(newest_field)
        .chain(entries.iter().map(|e| e.key.value.key_id))
        .chain(tombstones.iter().map(|t| t.stamp))
        .max()
}

#[cfg(test)]
mod tests;

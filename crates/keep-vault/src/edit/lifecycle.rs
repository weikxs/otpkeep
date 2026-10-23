//! Trash, permanent deletion, HOTP counters and key replacement.

use keep_passcode::{CodeKind, CodeSpec, SecretKey};

use super::{EditError, next_stamp, update_entry};
use crate::model::{Entry, EntryId, Extra, Field, Hlc, KeyMaterial, Tombstone, VaultDoc};

/// Entries stay in the trash for 30 days (PRD FR-4.3).
pub const TRASH_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Context for the deterministic id of an old-key copy.
const OLD_KEY_COPY_CONTEXT: &str = "otpkeep old key copy v1";

/// Moves an entry to the trash.
///
/// # Errors
///
/// Returns [`EditError::NotFound`] for an unknown entry.
pub fn move_to_trash(doc: &VaultDoc, id: EntryId, now_ms: u64) -> Result<VaultDoc, EditError> {
    update_entry(doc, id, now_ms, |entry, stamp| {
        Ok(Entry {
            trashed_at: Field {
                value: Some(stamp),
                stamp,
            },
            ..entry.clone()
        })
    })
}

/// Takes an entry out of the trash.
///
/// # Errors
///
/// Returns [`EditError::NotFound`] for an unknown entry.
pub fn restore(doc: &VaultDoc, id: EntryId, now_ms: u64) -> Result<VaultDoc, EditError> {
    update_entry(doc, id, now_ms, |entry, stamp| {
        Ok(Entry {
            trashed_at: Field { value: None, stamp },
            ..entry.clone()
        })
    })
}

/// Deletes an entry permanently. In a sync group a tombstone records the deletion, so sync does not bring
/// the entry back (design/06-sync.md §6.5).
///
/// # Errors
///
/// Returns [`EditError::NotFound`] for an unknown entry.
pub fn purge(doc: &VaultDoc, id: EntryId, now_ms: u64) -> Result<VaultDoc, EditError> {
    if !doc.entries.iter().any(|e| e.id == id) {
        return Err(EditError::NotFound);
    }
    Ok(delete_for_good(doc, &[id], now_ms))
}

/// Deletes entries that have been in the trash for longer than [`TRASH_RETENTION_MS`].
#[must_use]
pub fn purge_expired_trash(doc: &VaultDoc, now_ms: u64) -> VaultDoc {
    let expired: Vec<EntryId> = doc
        .entries
        .iter()
        .filter(|e| {
            e.trashed_at
                .value
                .is_some_and(|at| now_ms.saturating_sub(at.wall_ms) > TRASH_RETENTION_MS)
        })
        .map(|e| e.id)
        .collect();
    if expired.is_empty() {
        return doc.clone();
    }
    delete_for_good(doc, &expired, now_ms)
}

/// Removes `ids`, with a tombstone for each when the vault is in a sync group. The devices that hold a
/// tombstone are added when it is synced.
fn delete_for_good(doc: &VaultDoc, ids: &[EntryId], now_ms: u64) -> VaultDoc {
    let entries = doc
        .entries
        .iter()
        .filter(|e| !ids.contains(&e.id))
        .cloned()
        .collect();
    if doc.group.is_none() {
        return VaultDoc {
            entries,
            ..doc.clone()
        };
    }
    let mut clock_doc = doc.clone();
    let mut tombstones = doc.tombstones.clone();
    for id in ids {
        let stamp = next_stamp(&clock_doc, now_ms);
        clock_doc.clock = stamp;
        tombstones.retain(|t| t.entry != *id);
        tombstones.push(Tombstone {
            entry: *id,
            stamp,
            acked_by: Vec::new(),
        });
    }
    VaultDoc {
        entries,
        tombstones,
        clock: clock_doc.clock,
        ..doc.clone()
    }
}

/// Steps an HOTP entry's counter, which the user does by asking for the next code (PRD FR-1.5).
/// Returns the new counter.
///
/// # Errors
///
/// - [`EditError::NotFound`] for an unknown entry.
/// - [`EditError::NotHotp`] for a TOTP entry.
pub fn next_hotp(doc: &VaultDoc, id: EntryId, now_ms: u64) -> Result<(VaultDoc, u64), EditError> {
    let mut counter = 0;
    let doc = update_entry(doc, id, now_ms, |entry, stamp| {
        let key = &entry.key.value;
        if key.spec.kind() != CodeKind::CounterBased {
            return Err(EditError::NotHotp);
        }
        counter = key.counter.saturating_add(1);
        let value = KeyMaterial {
            counter,
            ..key.clone()
        };
        Ok(Entry {
            key: Field { value, stamp },
            ..entry.clone()
        })
    })?;
    Ok((doc, counter))
}

/// Gives an entry a new secret. The old secret is kept as a copy in the trash, so it is never lost
/// (design/06-sync.md §6.3).
///
/// # Errors
///
/// - [`EditError::NotFound`] for an unknown entry.
/// - [`EditError::Duplicate`] when another entry already has the new secret.
pub fn replace_key(
    doc: &VaultDoc,
    id: EntryId,
    secret: SecretKey,
    spec: CodeSpec,
    counter: u64,
    now_ms: u64,
) -> Result<VaultDoc, EditError> {
    let original = doc
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or(EditError::NotFound)?;
    if let Some(existing) = doc
        .entries
        .iter()
        .find(|e| e.id != id && e.key.value.secret.expose() == secret.expose())
    {
        return Err(EditError::Duplicate {
            existing: existing.id,
        });
    }
    let stamp = next_stamp(doc, now_ms);
    let copy = old_key_copy(original, stamp);
    let mut entries: Vec<Entry> = doc.entries.clone();
    if !entries.iter().any(|e| e.id == copy.id) {
        entries.push(copy);
    }
    let replaced = KeyMaterial {
        secret,
        spec,
        counter,
        key_id: stamp,
    };
    for entry in &mut entries {
        if entry.id == id {
            entry.key = Field {
                value: replaced.clone(),
                stamp,
            };
        }
    }
    Ok(VaultDoc {
        entries,
        clock: stamp,
        ..doc.clone()
    })
}

/// A trashed copy of `entry` holding its current key, stamped `stamp`. Its id depends only on the entry and
/// the key, so every device that makes this copy makes the same one; sync uses it for the key that loses a
/// merge (design/06-sync.md §6.3).
#[must_use]
pub fn old_key_copy(entry: &Entry, stamp: Hlc) -> Entry {
    let field = |value| Field { value, stamp };
    Entry {
        id: old_key_copy_id(entry.id, entry.key.value.key_id),
        created_at: stamp,
        title: field(entry.title.value.clone()),
        issuer: field(entry.issuer.value.clone()),
        account: field(entry.account.value.clone()),
        key: Field {
            value: entry.key.value.clone(),
            stamp,
        },
        note: field(entry.note.value.clone()),
        tags: Field {
            value: entry.tags.value.clone(),
            stamp,
        },
        rank: field(entry.rank.value.clone()),
        pinned: Field {
            value: false,
            stamp,
        },
        keep_duplicate: Field { value: true, stamp },
        trashed_at: Field {
            value: Some(stamp),
            stamp,
        },
        old_key_of: Field {
            value: Some(entry.id),
            stamp,
        },
        extra: Extra::new(),
    }
}

/// A version-8 (custom) UUID derived from the entry id and the key id.
fn old_key_copy_id(entry: EntryId, key_id: Hlc) -> EntryId {
    let mut hasher = blake3::Hasher::new_derive_key(OLD_KEY_COPY_CONTEXT);
    hasher.update(&entry.0);
    hasher.update(&key_id.wall_ms.to_be_bytes());
    hasher.update(&key_id.logical.to_be_bytes());
    hasher.update(&key_id.device.0);
    let mut id = [0u8; 16];
    id.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    id[6] = (id[6] & 0x0f) | 0x80;
    id[8] = (id[8] & 0x3f) | 0x80;
    EntryId(id)
}

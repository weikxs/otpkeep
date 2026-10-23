//! The order of entries: pinned first, then by rank key; and moving an entry.

use super::{EditError, update_entry};
use crate::model::{Entry, EntryId, Field, VaultDoc};
use crate::rank;

/// Where to move an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// To the top.
    First,
    /// To the bottom.
    Last,
    /// Just before another entry.
    Before(EntryId),
    /// Just after another entry.
    After(EntryId),
}

/// Entries not in the trash, in display order: pinned first, then by rank; ties by id, so every device agrees.
#[must_use]
pub fn ordered(doc: &VaultDoc) -> Vec<&Entry> {
    let mut entries: Vec<&Entry> = doc
        .entries
        .iter()
        .filter(|e| e.trashed_at.value.is_none())
        .collect();
    entries.sort_by(|a, b| {
        (!a.pinned.value, &a.rank.value, a.id).cmp(&(!b.pinned.value, &b.rank.value, b.id))
    });
    entries
}

/// Entries in the trash, most recently trashed first.
#[must_use]
pub fn trashed(doc: &VaultDoc) -> Vec<&Entry> {
    let mut entries: Vec<&Entry> = doc
        .entries
        .iter()
        .filter(|e| e.trashed_at.value.is_some())
        .collect();
    entries.sort_by(|a, b| {
        b.trashed_at
            .value
            .cmp(&a.trashed_at.value)
            .then(a.id.cmp(&b.id))
    });
    entries
}

/// Moves an entry; only its rank changes.
///
/// # Errors
///
/// Returns [`EditError::NotFound`] when the entry or the neighbour does not exist.
pub fn move_entry(
    doc: &VaultDoc,
    id: EntryId,
    placement: Placement,
    now_ms: u64,
) -> Result<VaultDoc, EditError> {
    if !doc.entries.iter().any(|e| e.id == id) {
        return Err(EditError::NotFound);
    }
    // The other entries in rank order (pinning does not change ranks).
    let mut others: Vec<&Entry> = doc
        .entries
        .iter()
        .filter(|e| e.id != id && e.trashed_at.value.is_none())
        .collect();
    others.sort_by(|a, b| (&a.rank.value, a.id).cmp(&(&b.rank.value, b.id)));
    let ranks: Vec<&str> = others.iter().map(|e| e.rank.value.as_str()).collect();
    let position = |target: EntryId| {
        others
            .iter()
            .position(|e| e.id == target)
            .ok_or(EditError::NotFound)
    };
    let new_rank = match placement {
        Placement::First => ranks.first().map_or_else(rank::first, |r| rank::before(r)),
        Placement::Last => ranks.last().map_or_else(rank::first, |r| rank::after(r)),
        Placement::Before(target) => {
            let at = position(target)?;
            gap(at.checked_sub(1).map(|i| ranks[i]), Some(ranks[at]))
        }
        Placement::After(target) => {
            let at = position(target)?;
            gap(Some(ranks[at]), ranks.get(at + 1).copied())
        }
    };
    update_entry(doc, id, now_ms, |entry, stamp| {
        Ok(Entry {
            rank: Field {
                value: new_rank,
                stamp,
            },
            ..entry.clone()
        })
    })
}

/// The rank of the last entry in rank order, if any.
pub(super) fn last_rank(doc: &VaultDoc) -> Option<String> {
    doc.entries.iter().map(|e| &e.rank.value).max().cloned()
}

/// A key between two neighbours (either may be missing). If they tie, which only happens after a sync merge,
/// the key goes just after the lower one and ids decide the order.
fn gap(low: Option<&str>, high: Option<&str>) -> String {
    match (low, high) {
        (Some(low), Some(high)) => rank::between(low, high).unwrap_or_else(|_| rank::after(low)),
        (Some(low), None) => rank::after(low),
        (None, Some(high)) => rank::before(high),
        (None, None) => rank::first(),
    }
}

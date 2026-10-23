//! The state digest both sides compare at the end of a sync, and the guard against clocks far ahead
//! (design/06-sync.md §6.6–6.7).

use keep_vault::{
    Entry, Hlc, MemberEvent, Tombstone, VaultDoc, encode_member_kind, encode_sync_state,
};

/// Domain separation for the digest.
const STATE_CONTEXT: &str = "otpkeep state v1";
/// Timestamps further ahead of this device's clock stop the session (design/10-reference.md §2).
pub const CLOCK_SKEW_LIMIT_MS: u64 = 24 * 60 * 60 * 1000;

/// A hash of everything that syncs: entries and tombstones by id, membership events by content. Equal digests
/// mean equal data.
#[must_use]
pub fn state_digest(doc: &VaultDoc) -> [u8; 32] {
    let mut entries: Vec<&Entry> = doc.entries.iter().collect();
    entries.sort_by_key(|e| e.id);
    let mut tombstones: Vec<Tombstone> = doc.tombstones.clone();
    tombstones.sort_by_key(|t| t.entry);
    for tomb in &mut tombstones {
        tomb.acked_by.sort_unstable();
        tomb.acked_by.dedup();
    }
    let mut events: Vec<&MemberEvent> = doc.group.iter().flat_map(|g| &g.events).collect();
    events.sort_by_cached_key(|e| (e.stamp, e.author, encode_member_kind(&e.kind), e.signature));
    events.dedup();
    let tombstones: Vec<&Tombstone> = tombstones.iter().collect();
    let state = encode_sync_state(&entries, &tombstones, &events);
    let mut hasher = blake3::Hasher::new_derive_key(STATE_CONTEXT);
    hasher.update(&state);
    *hasher.finalize().as_bytes()
}

/// A peer's timestamp is too far ahead of this device's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the other device's clock is more than a day ahead")]
pub struct PeerClock {
    /// The offending timestamp.
    pub stamp: Hlc,
}

/// Checks every timestamp in what a peer sent. Accepting a far-future timestamp would let that device win
/// every later conflict, so the whole session stops instead.
///
/// # Errors
///
/// Returns [`PeerClock`] with the first timestamp more than [`CLOCK_SKEW_LIMIT_MS`] ahead of `now_ms`.
pub fn check_peer_clock(received: &VaultDoc, now_ms: u64) -> Result<(), PeerClock> {
    let limit = now_ms.saturating_add(CLOCK_SKEW_LIMIT_MS);
    let entry_stamps = received.entries.iter().flat_map(entry_stamps);
    let tomb_stamps = received.tombstones.iter().map(|t| t.stamp);
    let group_stamps = received.group.iter().flat_map(|g| {
        g.events
            .iter()
            .map(|e| e.stamp)
            .chain(g.presence.values().copied())
    });
    match entry_stamps
        .chain(tomb_stamps)
        .chain(group_stamps)
        .find(|s| s.wall_ms > limit)
    {
        Some(stamp) => Err(PeerClock { stamp }),
        None => Ok(()),
    }
}

fn entry_stamps(entry: &Entry) -> Vec<Hlc> {
    let mut stamps = vec![
        entry.created_at,
        entry.title.stamp,
        entry.issuer.stamp,
        entry.account.stamp,
        entry.key.stamp,
        entry.key.value.key_id,
        entry.note.stamp,
        entry.tags.stamp,
        entry.rank.stamp,
        entry.pinned.stamp,
        entry.keep_duplicate.stamp,
        entry.trashed_at.stamp,
        entry.old_key_of.stamp,
    ];
    stamps.extend(entry.trashed_at.value);
    stamps
}

#[cfg(test)]
mod tests;

//! The data phase of a sync session, without the network (design/06-sync.md §5.4 step ⑥).
//!
//! Both sides' complete states are known here; the networked session exchanges only what each side lacks and
//! then calls this to compute the results both sides commit. Nothing is written unless everything succeeds.

use keep_vault::history::HistoryField;
use keep_vault::{DeviceId, EntryId, Hlc, VaultDoc};

use crate::digest::{PeerClock, check_peer_clock, state_digest};
use crate::housekeeping::dedupe;
use crate::membership::{membership, merge_events};
use crate::merge::merge;

/// Why a session stopped. Neither side changes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SyncStop {
    /// One side is not in a sync group, or the two are in different groups.
    #[error("the devices are not in the same sync group")]
    OtherGroup,
    /// The device is not a member (`NOT_IN_GROUP`).
    #[error("the device is not in the sync group")]
    NotInGroup(DeviceId),
    /// The device was removed (`REMOVED`).
    #[error("the device was removed from the sync group")]
    Removed(DeviceId),
    /// A timestamp from the other side is far ahead (`PEER_CLOCK`).
    #[error(transparent)]
    PeerClock(#[from] PeerClock),
    /// The two sides computed different results: a defect (`DIGEST_MISMATCH`).
    #[error("the two devices computed different results")]
    DigestMismatch,
}

/// What both sides commit.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncDone {
    /// The opener's new state.
    pub opener: VaultDoc,
    /// The joiner's new state.
    pub joiner: VaultDoc,
    /// Entries moved to the trash as duplicates.
    pub deduped: Vec<EntryId>,
    /// The state digest both sides agreed on.
    pub digest: [u8; 32],
    /// Names and notes on the opener that the merge replaced, for its edit history.
    pub opener_overwritten: Vec<(EntryId, HistoryField, String)>,
    /// The same for the joiner.
    pub joiner_overwritten: Vec<(EntryId, HistoryField, String)>,
}

/// Merges the two states: membership first, then the clock check, then entries; the opener removes duplicate
/// keys and the joiner takes its result; both record the sync in `presence`.
///
/// # Errors
///
/// Returns [`SyncStop`]; nothing is to be written then.
pub fn sync_data(
    opener: &VaultDoc,
    joiner: &VaultDoc,
    opener_id: DeviceId,
    joiner_id: DeviceId,
    opener_now_ms: u64,
    joiner_now_ms: u64,
) -> Result<SyncDone, SyncStop> {
    check_members(opener, joiner, opener_id, joiner_id)?;
    check_peer_clock(joiner, opener_now_ms)?;
    check_peer_clock(opener, joiner_now_ms)?;
    let participants = [opener_id, joiner_id];
    let merged = merge(opener, joiner, &participants);
    let (merged, deduped) = dedupe(&merged, opener_now_ms);
    let stamp = next_stamp(&merged, opener_now_ms);
    let opener_new = with_presence(merged, &participants, stamp);
    let joiner_new = with_presence(
        merge(joiner, &opener_new, &participants),
        &participants,
        stamp,
    );
    let digest = state_digest(&opener_new);
    if state_digest(&joiner_new) != digest {
        tracing::error!("state digests differ after merging");
        return Err(SyncStop::DigestMismatch);
    }
    Ok(SyncDone {
        opener_overwritten: overwritten(opener, &opener_new),
        joiner_overwritten: overwritten(joiner, &joiner_new),
        opener: opener_new,
        joiner: joiner_new,
        deduped,
        digest,
    })
}

/// When a session stops with [`SyncStop::Removed`], what each side keeps: its own state with the membership
/// events of both. The events that decide a removal may sit on one side only, and the identity check refuses
/// before anything else is exchanged; keeping them lets every device learn of the removal. Nothing else
/// changes. The events are signed, and the other side's new ones are checked.
#[must_use]
pub fn share_membership(opener: &VaultDoc, joiner: &VaultDoc) -> (VaultDoc, VaultDoc) {
    let with_events = |ours: &VaultDoc, theirs: &VaultDoc| {
        let mut doc = ours.clone();
        if let (Some(group), Some(other)) = (doc.group.as_mut(), theirs.group.as_ref())
            && group.group_id == other.group_id
        {
            group.events = merge_events(&group.group_id, &group.events, &other.events);
        }
        doc
    };
    (with_events(opener, joiner), with_events(joiner, opener))
}

/// Both must be current members of the same group, by the union of what both know.
fn check_members(
    opener: &VaultDoc,
    joiner: &VaultDoc,
    opener_id: DeviceId,
    joiner_id: DeviceId,
) -> Result<(), SyncStop> {
    let (Some(ours), Some(theirs)) = (&opener.group, &joiner.group) else {
        return Err(SyncStop::OtherGroup);
    };
    if ours.group_id != theirs.group_id {
        return Err(SyncStop::OtherGroup);
    }
    let group = keep_vault::SyncGroup {
        events: merge_events(&ours.group_id, &ours.events, &theirs.events),
        ..ours.clone()
    };
    let members = membership(&group);
    for device in [opener_id, joiner_id] {
        if members.is_removed(device) {
            return Err(SyncStop::Removed(device));
        }
        if !members.is_member(device) {
            return Err(SyncStop::NotInGroup(device));
        }
    }
    Ok(())
}

/// A timestamp later than anything in `doc`, from the opener.
pub(crate) fn next_stamp(doc: &VaultDoc, now_ms: u64) -> Hlc {
    let wall_ms = now_ms.max(doc.clock.wall_ms);
    let logical = if wall_ms == doc.clock.wall_ms {
        doc.clock.logical.saturating_add(1)
    } else {
        0
    };
    Hlc {
        wall_ms,
        logical,
        device: doc.device_tag,
    }
}

pub(crate) fn with_presence(mut doc: VaultDoc, devices: &[DeviceId], stamp: Hlc) -> VaultDoc {
    if let Some(group) = doc.group.as_mut() {
        for device in devices {
            let seen = group.presence.entry(*device).or_insert(stamp);
            *seen = (*seen).max(stamp);
        }
    }
    doc.clock = doc.clock.max(stamp);
    doc
}

/// Names and notes in `before` that `after` replaced.
pub(crate) fn overwritten(
    before: &VaultDoc,
    after: &VaultDoc,
) -> Vec<(EntryId, HistoryField, String)> {
    let mut changes = Vec::new();
    for old in &before.entries {
        let Some(new) = after.entries.iter().find(|e| e.id == old.id) else {
            continue;
        };
        if new.title.value != old.title.value {
            changes.push((old.id, HistoryField::Title, old.title.value.clone()));
        }
        if new.note.value != old.note.value {
            changes.push((old.id, HistoryField::Note, old.note.value.clone()));
        }
    }
    changes
}

pub mod data;
pub mod exchange;

#[cfg(test)]
mod tests;

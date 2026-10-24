//! Membership events and the member list (design/06-sync.md §1.2–1.3).
//!
//! Events merge as a set, and every device computes the members from the set with the same rules, so devices
//! that hold the same events agree. The rules run over the events in one total order — by timestamp, then
//! author, then content — so the result does not depend on the order events arrived in:
//!
//! - the earliest self-join founds the group; other self-joins count for nothing;
//! - a join counts when its author is a member at that point and its certificate binds the joining device's
//!   exchange key; a removed device never joins again;
//! - any member may remove any member, permanently;
//! - a device may only rename itself, and its latest name wins; names are applied after joins and removals,
//!   since a new device names itself with its own clock, and count only for current members;
//! - members added by a device that was later removed are marked, so the user can decide about them.
//!
//! Events signed by a removed device after its removal can still reach the group through a device that had
//! not heard of the removal. They are kept, so every device converges on the same set, but they count for
//! nothing once they sort after the removal.

use std::collections::{BTreeMap, BTreeSet};

use keep_vault::{DeviceId, Hlc, MemberEvent, MemberKind, Platform, SyncGroup, encode_member_kind};

use crate::identity::{DeviceIdentity, verify, verify_dx_cert};

/// The most devices a group may have (design/10-reference.md §2).
pub const MAX_GROUP_MEMBERS: usize = 16;

/// Domain separation for event signatures.
const EVENT_CONTEXT: &[u8] = b"otpkeep member v1";

/// A device in the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its identity.
    pub device: DeviceId,
    /// Its X25519 key for handshakes.
    pub dx_public: [u8; 32],
    /// Its platform.
    pub platform: Platform,
    /// Its latest LAN name, if it has set one.
    pub lan_name: Option<String>,
    /// The member that added it; the founder added itself.
    pub added_by: DeviceId,
    /// The member that added it has since been removed: ask the user whether to keep it (08-security T13).
    pub added_by_removed: bool,
    /// When it was added.
    pub joined_at: Hlc,
}

/// Who is in the group, as computed from its events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Membership {
    /// The device that created the group.
    pub founder: Option<DeviceId>,
    /// Current members.
    pub members: BTreeMap<DeviceId, Member>,
    /// Devices removed for good.
    pub removed: BTreeSet<DeviceId>,
    /// Members that were later removed.
    pub former: BTreeSet<DeviceId>,
}

impl Membership {
    /// Whether `device` is a current member.
    #[must_use]
    pub fn is_member(&self, device: DeviceId) -> bool {
        self.members.contains_key(&device)
    }

    /// Whether `device` was removed.
    #[must_use]
    pub fn is_removed(&self, device: DeviceId) -> bool {
        self.removed.contains(&device)
    }

    /// The devices that must hold a tombstone before it may be cleared: every device that ever joined.
    ///
    /// Removed devices still count. A device that learned of a removal may not know of an earlier removal of
    /// that removal's author — the removed device itself may hold it — which would make the removed device a
    /// member again, still holding the deleted entry. No device can rule this out, so a removal never
    /// releases a tombstone; the cost is a few bytes per deletion.
    #[must_use]
    pub fn required_for_cleanup(&self) -> BTreeSet<DeviceId> {
        self.members.keys().chain(&self.former).copied().collect()
    }

    /// Whether no more devices may be paired.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.members.len() >= MAX_GROUP_MEMBERS
    }
}

/// Signs a new event as `author`.
#[must_use]
pub fn sign_event(
    author: &DeviceIdentity,
    group_id: &[u8; 16],
    kind: MemberKind,
    stamp: Hlc,
) -> MemberEvent {
    let signature = author.sign(&event_message(group_id, author.device_id(), &kind, stamp));
    MemberEvent {
        kind,
        author: author.device_id(),
        stamp,
        signature,
    }
}

/// Whether the event is signed by its author for this group, and a join's certificate is valid.
#[must_use]
pub fn verify_event(group_id: &[u8; 16], event: &MemberEvent) -> bool {
    let message = event_message(group_id, event.author, &event.kind, event.stamp);
    if !verify(event.author, &message, &event.signature) {
        return false;
    }
    match &event.kind {
        MemberKind::Join {
            device,
            dx_public,
            dx_cert,
            ..
        } => verify_dx_cert(*device, group_id, dx_public, dx_cert),
        _ => true,
    }
}

/// What a signature covers: context, group, author, time and the canonical CBOR of the kind.
fn event_message(group_id: &[u8; 16], author: DeviceId, kind: &MemberKind, stamp: Hlc) -> Vec<u8> {
    [
        EVENT_CONTEXT,
        group_id,
        &author.0,
        &stamp.wall_ms.to_be_bytes(),
        &stamp.logical.to_be_bytes(),
        &stamp.device.0,
        &encode_member_kind(kind),
    ]
    .concat()
}

/// The union of our events and theirs, in canonical order. Ours were checked when they were stored; each of
/// theirs that is new here is checked now, and dropped when its signature or certificate is invalid.
#[must_use]
pub fn merge_events(
    group_id: &[u8; 16],
    ours: &[MemberEvent],
    theirs: &[MemberEvent],
) -> Vec<MemberEvent> {
    let mut merged: BTreeMap<SortKey, MemberEvent> =
        ours.iter().map(|e| (sort_key(e), e.clone())).collect();
    for event in theirs {
        let key = sort_key(event);
        if merged.contains_key(&key) {
            continue;
        }
        if !verify_event(group_id, event) {
            tracing::warn!("dropped a membership event with an invalid signature");
            continue;
        }
        merged.insert(key, event.clone());
    }
    merged.into_values().collect()
}

/// Each device's later last-sync time.
#[must_use]
pub fn merge_presence(
    ours: &BTreeMap<DeviceId, Hlc>,
    theirs: &BTreeMap<DeviceId, Hlc>,
) -> BTreeMap<DeviceId, Hlc> {
    let mut merged = ours.clone();
    for (device, seen) in theirs {
        merged
            .entry(*device)
            .and_modify(|current| *current = (*current).max(*seen))
            .or_insert(*seen);
    }
    merged
}

/// Orders events totally: time, author, content, signature.
type SortKey = (Hlc, DeviceId, Vec<u8>, [u8; 64]);

fn sort_key(event: &MemberEvent) -> SortKey {
    (
        event.stamp,
        event.author,
        encode_member_kind(&event.kind),
        event.signature,
    )
}

/// The members, computed from the group's events.
///
/// Stored events are trusted: every event enters a group either through [`merge_events`], which checks it,
/// or through [`sign_event`] on this device, and the vault that holds them is authenticated.
#[must_use]
pub fn membership(group: &SyncGroup) -> Membership {
    let mut events: Vec<(SortKey, &MemberEvent)> =
        group.events.iter().map(|e| (sort_key(e), e)).collect();
    events.sort_by(|a, b| a.0.cmp(&b.0));
    let mut state = Membership::default();
    for (_, event) in &events {
        apply(&mut state, event);
    }
    // Names last: a new device signs its name with its own clock, which may sort before the join another
    // device stamped. Only current members' names count; the latest in the order wins.
    for (_, event) in &events {
        if let MemberKind::Rename { device, lan_name } = &event.kind
            && *device == event.author
            && let Some(member) = state.members.get_mut(device)
        {
            member.lan_name = Some(lan_name.clone());
        }
    }
    let removed = state.removed.clone();
    for member in state.members.values_mut() {
        member.added_by_removed = removed.contains(&member.added_by);
    }
    state
}

fn apply(state: &mut Membership, event: &MemberEvent) {
    let author_is_member = state.is_member(event.author);
    match &event.kind {
        MemberKind::Join {
            device,
            dx_public,
            platform,
            ..
        } => {
            let founding = state.founder.is_none() && *device == event.author;
            if (founding || author_is_member)
                && !state.is_removed(*device)
                && !state.is_member(*device)
            {
                if founding {
                    state.founder = Some(*device);
                }
                state.members.insert(
                    *device,
                    Member {
                        device: *device,
                        dx_public: *dx_public,
                        platform: *platform,
                        lan_name: None,
                        added_by: event.author,
                        added_by_removed: false,
                        joined_at: event.stamp,
                    },
                );
            }
        }
        MemberKind::Remove { device } => {
            if author_is_member {
                if state.members.remove(device).is_some() {
                    state.former.insert(*device);
                }
                state.removed.insert(*device);
            }
        }
        MemberKind::Rename { .. } | MemberKind::Other { .. } => {}
    }
}

#[cfg(test)]
mod tests;

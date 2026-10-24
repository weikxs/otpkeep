//! Managing the group from one device: removing a member, leaving, renaming, and the devices page
//! (design/06-sync.md §2, §5.5, §7).

use keep_vault::{DeviceId, DeviceTag, MemberKind, Platform, VaultDoc};

use super::{GroupError, identity_of, rename_event, tick};
use crate::channel::hello::is_valid_lan_name;
use crate::identity::full_fingerprint;
use crate::membership::{membership, sign_event};

/// `PEER_STALE_WARN`: a member not synced for this long may be behind.
pub const PEER_STALE_WARN_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// `PEER_STALE_REMOVE_HINT`: a member not synced for this long is suggested for removal.
pub const PEER_STALE_REMOVE_HINT_MS: u64 = 90 * 24 * 60 * 60 * 1000;

/// Signs the removal of `device`, which is permanent; it spreads with the next syncs. A device may remove itself.
///
/// # Errors
///
/// Returns [`GroupError::NotInGroup`], [`GroupError::Removed`] or [`GroupError::NotAMember`].
pub fn remove_member(
    doc: &VaultDoc,
    device: DeviceId,
    now_ms: u64,
) -> Result<VaultDoc, GroupError> {
    let group = doc.group.as_ref().ok_or(GroupError::NotInGroup)?;
    let members = membership(group);
    if members.is_removed(device) {
        return Err(GroupError::Removed);
    }
    if !members.is_member(device) {
        return Err(GroupError::NotAMember);
    }
    let identity = identity_of(doc)?;
    let group_id = group.group_id;
    let mut changed = doc.clone();
    let removal = sign_event(
        &identity,
        &group_id,
        MemberKind::Remove { device },
        tick(&mut changed, now_ms),
    );
    if let Some(group) = changed.group.as_mut() {
        group.events.push(removal);
    }
    tracing::info!("signed a member removal");
    Ok(changed)
}

/// Leaves the group: forgets it and this device's identity and keeps the entries as a vault of its own. Other
/// devices cannot be told; the person removes this device there.
///
/// # Errors
///
/// Returns [`GroupError::NotInGroup`] or [`GroupError::Random`].
pub fn leave_group(doc: &VaultDoc) -> Result<VaultDoc, GroupError> {
    if doc.group.is_none() {
        return Err(GroupError::NotInGroup);
    }
    let mut tag = [0u8; 8];
    getrandom::fill(&mut tag).map_err(|_| GroupError::Random)?;
    Ok(VaultDoc {
        group: None,
        device: None,
        tombstones: Vec::new(),
        device_tag: DeviceTag(tag),
        ..doc.clone()
    })
}

/// Renames this device in its group.
///
/// # Errors
///
/// Returns [`GroupError::NotInGroup`] or [`GroupError::BadName`].
pub fn rename_self(doc: &VaultDoc, lan_name: &str, now_ms: u64) -> Result<VaultDoc, GroupError> {
    let group = doc.group.as_ref().ok_or(GroupError::NotInGroup)?;
    if !is_valid_lan_name(lan_name) {
        return Err(GroupError::BadName);
    }
    let identity = identity_of(doc)?;
    let group_id = group.group_id;
    let mut changed = doc.clone();
    let rename = rename_event(&identity, &group_id, lan_name, tick(&mut changed, now_ms));
    if let Some(group) = changed.group.as_mut() {
        group.events.push(rename);
    }
    Ok(changed)
}

/// How long ago a member last synced, as the devices page shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// Synced recently, or this device.
    Fresh,
    /// Not synced for 30 days: "may be behind".
    MayBeBehind,
    /// Not synced for 90 days: "consider removing".
    ConsiderRemoving,
}

/// One row of the devices page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRow {
    /// The device.
    pub device: DeviceId,
    /// Its LAN name.
    pub lan_name: Option<String>,
    /// Its platform.
    pub platform: Platform,
    /// Its full fingerprint.
    pub fingerprint: String,
    /// When it last finished a sync, as far as this device knows.
    pub last_sync_ms: Option<u64>,
    /// How long ago that was.
    pub staleness: Staleness,
    /// The member that added it has been removed: ask the person whether to keep it.
    pub added_by_removed: bool,
    /// This device.
    pub is_this_device: bool,
}

/// The members, for the devices page. A member that never synced counts from when it joined.
///
/// # Errors
///
/// Returns [`GroupError::NotInGroup`].
pub fn device_rows(doc: &VaultDoc, now_ms: u64) -> Result<Vec<DeviceRow>, GroupError> {
    let group = doc.group.as_ref().ok_or(GroupError::NotInGroup)?;
    let me = identity_of(doc)?.device_id();
    let members = membership(group);
    Ok(members
        .members
        .values()
        .map(|member| {
            let last_sync_ms = group.presence.get(&member.device).map(|seen| seen.wall_ms);
            let since = now_ms.saturating_sub(last_sync_ms.unwrap_or(member.joined_at.wall_ms));
            let is_this_device = member.device == me;
            let staleness = if is_this_device || since < PEER_STALE_WARN_MS {
                Staleness::Fresh
            } else if since < PEER_STALE_REMOVE_HINT_MS {
                Staleness::MayBeBehind
            } else {
                Staleness::ConsiderRemoving
            };
            DeviceRow {
                device: member.device,
                lan_name: member.lan_name.clone(),
                platform: member.platform,
                fingerprint: full_fingerprint(member.device),
                last_sync_ms,
                staleness,
                added_by_removed: member.added_by_removed,
                is_this_device,
            }
        })
        .collect())
}

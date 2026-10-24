//! Founding a sync group, admitting a device and joining (design/06-sync.md §4.1, §4.3 ⑥).

use keep_vault::{DeviceId, Hlc, MemberEvent, MemberKind, Platform, SyncGroup, VaultDoc};

use crate::identity::{DeviceIdentity, IdentityError, NewDevice, device_tag, verify_dx_cert};
use crate::membership::{membership, sign_event};

/// Why a group step was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GroupError {
    /// This device is already in a group (`OTHER_GROUP`).
    #[error("this device is already in a sync group")]
    AlreadyInGroup,
    /// This device is not in a group.
    #[error("this device is not in a sync group")]
    NotInGroup,
    /// The group has 16 devices (`GROUP_FULL`).
    #[error("the sync group has the most devices allowed")]
    Full,
    /// The device is already a member (`ALREADY_MEMBER`).
    #[error("the device is already in the sync group")]
    AlreadyMember,
    /// The device was removed (`REMOVED`).
    #[error("the device was removed from the sync group")]
    Removed,
    /// The certificate does not bind the key the device used in the handshake.
    #[error("the device's certificate does not match its connection")]
    BadCertificate,
    /// No random bytes.
    #[error("the system random number generator is unavailable")]
    Random,
}

impl From<IdentityError> for GroupError {
    fn from(_: IdentityError) -> Self {
        Self::Random
    }
}

/// Creates a group with this device as its first member, named `lan_name`. Entries stay as they are.
///
/// # Errors
///
/// Returns [`GroupError::AlreadyInGroup`] or [`GroupError::Random`].
pub fn found_group(
    doc: &VaultDoc,
    platform: Platform,
    lan_name: &str,
    now_ms: u64,
) -> Result<VaultDoc, GroupError> {
    if doc.group.is_some() {
        return Err(GroupError::AlreadyInGroup);
    }
    let mut group_id = [0u8; 16];
    let mut tag_key = [0u8; 32];
    getrandom::fill(&mut group_id).map_err(|_| GroupError::Random)?;
    getrandom::fill(&mut tag_key).map_err(|_| GroupError::Random)?;
    let identity = NewDevice::generate()?.into_identity(&group_id);
    let me = identity.device_id();
    let mut grouped = VaultDoc {
        device_tag: device_tag(me),
        device: Some(identity.secrets()),
        ..doc.clone()
    };
    let join = sign_event(
        &identity,
        &group_id,
        MemberKind::Join {
            device: me,
            dx_public: identity.dx_public(),
            dx_cert: identity.dx_cert(),
            platform,
        },
        tick(&mut grouped, now_ms),
    );
    let rename = rename_event(&identity, &group_id, lan_name, tick(&mut grouped, now_ms));
    grouped.group = Some(SyncGroup {
        group_id,
        tag_key,
        events: vec![join, rename],
        presence: std::collections::BTreeMap::new(),
        extra: keep_vault::Extra::new(),
    });
    Ok(grouped)
}

/// This device's identity in its group.
///
/// # Errors
///
/// Returns [`GroupError::NotInGroup`].
pub fn identity_of(doc: &VaultDoc) -> Result<DeviceIdentity, GroupError> {
    match (&doc.group, &doc.device) {
        (Some(group), Some(secrets)) => Ok(DeviceIdentity::from_secrets(secrets, &group.group_id)),
        _ => Err(GroupError::NotInGroup),
    }
}

/// Signs `Join` for a new device that proved `handshake_key` in the handshake and certified it for this group.
///
/// # Errors
///
/// Returns [`GroupError`].
pub fn admit(
    doc: &VaultDoc,
    device: DeviceId,
    handshake_key: [u8; 32],
    dx_cert: &[u8; 64],
    platform: Platform,
    now_ms: u64,
) -> Result<VaultDoc, GroupError> {
    let group = doc.group.as_ref().ok_or(GroupError::NotInGroup)?;
    let members = membership(group);
    if members.is_removed(device) {
        return Err(GroupError::Removed);
    }
    if members.is_member(device) {
        return Err(GroupError::AlreadyMember);
    }
    if members.is_full() {
        return Err(GroupError::Full);
    }
    if !verify_dx_cert(device, &group.group_id, &handshake_key, dx_cert) {
        return Err(GroupError::BadCertificate);
    }
    let identity = identity_of(doc)?;
    let mut admitted = doc.clone();
    let join = sign_event(
        &identity,
        &group.group_id,
        MemberKind::Join {
            device,
            dx_public: handshake_key,
            dx_cert: *dx_cert,
            platform,
        },
        tick(&mut admitted, now_ms),
    );
    if let Some(group) = admitted.group.as_mut() {
        group.events.push(join);
    }
    Ok(admitted)
}

/// This device joins `group` as `identity`, naming itself `lan_name`. With `replace`, its entries are dropped
/// first (the caller keeps a backup); otherwise they are merged in the data phase.
///
/// # Errors
///
/// Returns [`GroupError::AlreadyInGroup`].
pub fn join_group(
    doc: &VaultDoc,
    identity: &DeviceIdentity,
    group: &SyncGroup,
    lan_name: &str,
    replace: bool,
    now_ms: u64,
) -> Result<VaultDoc, GroupError> {
    if doc.group.is_some() {
        return Err(GroupError::AlreadyInGroup);
    }
    let mut joined = VaultDoc {
        device_tag: device_tag(identity.device_id()),
        device: Some(identity.secrets()),
        ..doc.clone()
    };
    if replace {
        joined.entries.clear();
        joined.tombstones.clear();
    }
    let rename = rename_event(
        identity,
        &group.group_id,
        lan_name,
        tick(&mut joined, now_ms),
    );
    let mut events = group.events.clone();
    events.push(rename);
    joined.group = Some(SyncGroup {
        events,
        ..group.clone()
    });
    Ok(joined)
}

fn rename_event(
    identity: &DeviceIdentity,
    group_id: &[u8; 16],
    lan_name: &str,
    stamp: Hlc,
) -> MemberEvent {
    sign_event(
        identity,
        group_id,
        MemberKind::Rename {
            device: identity.device_id(),
            lan_name: lan_name.to_owned(),
        },
        stamp,
    )
}

/// A timestamp later than anything in `doc`, which then counts as seen.
fn tick(doc: &mut VaultDoc, now_ms: u64) -> Hlc {
    let stamp = crate::session::next_stamp(doc, now_ms);
    doc.clock = stamp;
    stamp
}

#[cfg(test)]
mod tests;

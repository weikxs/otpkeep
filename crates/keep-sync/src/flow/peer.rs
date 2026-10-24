//! Checking who is on the other end of a sync handshake (design/06-sync.md §5.4 ②).

use keep_vault::{DeviceId, Platform, VaultDoc};

use crate::channel::PROTOCOL_VERSION;
use crate::channel::hello::{Hello, common_version};
use crate::channel::message::ErrorCode;
use crate::identity::full_fingerprint;
use crate::membership::membership;

/// The other device, as shown to the person deciding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// Its identity.
    pub device: DeviceId,
    /// Its LAN name.
    pub lan_name: String,
    /// Its platform.
    pub platform: Platform,
    /// Its full fingerprint, for the "allow" prompt.
    pub fingerprint: String,
}

/// The peer must speak a common version, be a member of our group by what this device knows, and have
/// proved the exchange key its join recorded. Neither failure counts as a wrong code.
///
/// # Errors
///
/// Returns the code to send: `VERSION_MISMATCH`, `REMOVED` or `NOT_IN_GROUP`.
pub fn check_member(
    doc: &VaultDoc,
    hello: &Hello,
    remote_static: [u8; 32],
) -> Result<PeerInfo, ErrorCode> {
    if common_version(&[PROTOCOL_VERSION], &hello.versions).is_none() {
        return Err(ErrorCode::VersionMismatch);
    }
    let group = doc.group.as_ref().ok_or(ErrorCode::NotInGroup)?;
    let members = membership(group);
    if members.is_removed(hello.device) {
        return Err(ErrorCode::Removed);
    }
    let member = members
        .members
        .get(&hello.device)
        .ok_or(ErrorCode::NotInGroup)?;
    if member.dx_public != remote_static {
        return Err(ErrorCode::NotInGroup);
    }
    Ok(PeerInfo {
        device: hello.device,
        lan_name: hello.lan_name.clone(),
        platform: member.platform,
        fingerprint: full_fingerprint(hello.device),
    })
}

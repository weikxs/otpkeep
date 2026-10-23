//! The sync group and this device's sync identity, as stored in the vault (design/06-sync.md §1).
//!
//! Only the data lives here; signing, checking and merging are in `keep-sync`.

use std::collections::BTreeMap;
use std::fmt;

use ciborium::Value;
use zeroize::Zeroizing;

use super::{Extra, Hlc};

/// A device in a sync group: its Ed25519 public signing key.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub [u8; 32]);

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId(")?;
        self.0[..4].iter().try_for_each(|b| write!(f, "{b:02x}"))?;
        write!(f, "…)")
    }
}

/// The platform a device said it runs on when it joined; shown on the devices page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Platform {
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
    /// Linux.
    Linux,
    /// iOS or iPadOS.
    Ios,
    /// Android.
    Android,
    /// A platform a newer version knows.
    Other(u64),
}

/// What a membership event says.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberKind {
    /// `device` joins, with the X25519 key it uses in handshakes and the certificate binding that key to it.
    Join {
        /// The new member.
        device: DeviceId,
        /// Its X25519 static public key.
        dx_public: [u8; 32],
        /// Its signature over the group id and `dx_public`.
        dx_cert: [u8; 64],
        /// Its platform.
        platform: Platform,
    },
    /// `device` is removed for good.
    Remove {
        /// The removed member.
        device: DeviceId,
    },
    /// `device` takes a new LAN name.
    Rename {
        /// The renamed member, which must be the author.
        device: DeviceId,
        /// The new name.
        lan_name: String,
    },
    /// A kind from a newer version: kept, passed on, and ignored when computing members.
    Other {
        /// Its kind number.
        kind: u64,
        /// Its contents, exactly as received.
        body: Value,
    },
}

/// A signed change to the group's membership.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberEvent {
    /// What changes.
    pub kind: MemberKind,
    /// Who signed it.
    pub author: DeviceId,
    /// When.
    pub stamp: Hlc,
    /// The author's Ed25519 signature (design/06-sync.md §1.2).
    pub signature: [u8; 64],
}

/// The devices that sync with each other.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncGroup {
    /// Random, chosen when the group is created.
    pub group_id: [u8; 16],
    /// Random; only used to derive the group tag in LAN announcements.
    pub tag_key: [u8; 32],
    /// Every membership event known here; merged as a set.
    pub events: Vec<MemberEvent>,
    /// When each device last finished a sync; merged by keeping the later time. Not signed.
    pub presence: BTreeMap<DeviceId, Hlc>,
    /// Fields from newer versions.
    pub extra: Extra,
}

/// This device's private keys in its sync group. Never synced, and wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSecrets {
    /// The Ed25519 signing key seed.
    pub signing_seed: Zeroizing<[u8; 32]>,
    /// The X25519 static secret.
    pub exchange_secret: Zeroizing<[u8; 32]>,
}

impl fmt::Debug for DeviceSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceSecrets(…)")
    }
}

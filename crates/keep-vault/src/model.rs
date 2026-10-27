//! The vault's data model (design/02-data.md §1).

use std::collections::BTreeMap;
use std::fmt;

use ciborium::Value;
use keep_passcode::{CodeSpec, SecretKey};

mod group;

pub use group::{DeviceId, DeviceSecrets, MemberEvent, MemberKind, Platform, SyncGroup};

/// Fields this version does not know, keyed by their integer CBOR key. They are kept and written back so an
/// older version never loses what a newer one stored.
pub type Extra = BTreeMap<u64, Value>;

/// The device part of a timestamp: the first 8 bytes of a hash of the device's public key, or random bytes
/// on a device that has not joined a sync group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceTag(pub [u8; 8]);

/// A hybrid logical clock timestamp. Ordered by wall clock, then counter, then device, which makes the order
/// total, so every device resolves the same conflict the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hlc {
    /// Milliseconds since the Unix epoch.
    pub wall_ms: u64,
    /// Breaks ties between changes in the same millisecond.
    pub logical: u32,
    /// Breaks ties between devices.
    pub device: DeviceTag,
}

/// A value and the time it was last changed; fields merge by keeping the later stamp.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<T> {
    /// The value.
    pub value: T,
    /// When it was last set.
    pub stamp: Hlc,
}

/// A random version 4 UUID that identifies an entry on every device.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId(pub [u8; 16]);

impl EntryId {
    /// A new random identifier.
    #[must_use]
    pub fn random() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    /// Reads an identifier in the form [`EntryId`]'s `Display` writes; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        uuid::Uuid::try_parse(text)
            .ok()
            .map(|uuid| Self(*uuid.as_bytes()))
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        uuid::Uuid::from_bytes(self.0).hyphenated().fmt(f)
    }
}

impl fmt::Debug for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntryId({self})")
    }
}

/// The secret, how codes are computed, and the HOTP counter. They always change together, so a merge can never
/// combine a new secret with old parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    /// The shared secret.
    pub secret: SecretKey,
    /// Kind, algorithm, digits and period.
    pub spec: CodeSpec,
    /// The HOTP counter; 0 for TOTP.
    pub counter: u64,
    /// When this secret was set (added or replaced).
    pub key_id: Hlc,
}

/// One two-factor account.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Stable identifier.
    pub id: EntryId,
    /// When the entry was created.
    pub created_at: Hlc,
    /// Display name.
    pub title: Field<String>,
    /// Service name, possibly empty.
    pub issuer: Field<String>,
    /// Account name, possibly empty.
    pub account: Field<String>,
    /// Secret, code specification and counter.
    pub key: Field<KeyMaterial>,
    /// Free-form note.
    pub note: Field<String>,
    /// Tags, in the order the user added them.
    pub tags: Field<Vec<String>>,
    /// Fractional-index sort key.
    pub rank: Field<String>,
    /// Shown first.
    pub pinned: Field<bool>,
    /// The user chose "add anyway" for a duplicate secret; sync never merges it away.
    pub keep_duplicate: Field<bool>,
    /// When it was moved to the trash; `None` when it is not in the trash.
    pub trashed_at: Field<Option<Hlc>>,
    /// Set on the copy that keeps a replaced secret: the entry whose old key this is.
    pub old_key_of: Field<Option<EntryId>>,
    /// Fields from newer versions.
    pub extra: Extra,
}

/// The record of a permanently deleted entry, so sync does not bring it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tombstone {
    /// The deleted entry.
    pub entry: EntryId,
    /// When it was deleted.
    pub stamp: Hlc,
    /// Devices known to hold this tombstone (public keys).
    pub acked_by: Vec<[u8; 32]>,
}

/// Everything stored in `vault.okv`.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultDoc {
    /// Identifies this vault.
    pub vault_id: [u8; 16],
    /// All entries, including those in the trash.
    pub entries: Vec<Entry>,
    /// Permanently deleted entries (sync groups only).
    pub tombstones: Vec<Tombstone>,
    /// The sync group, once this device has created or joined one.
    pub group: Option<SyncGroup>,
    /// This device's keys in the group; present exactly when `group` is. Never synced.
    pub device: Option<DeviceSecrets>,
    /// The latest timestamp seen; new stamps are always later.
    pub clock: Hlc,
    /// This device's part of new timestamps. Random until the device joins a sync group.
    pub device_tag: DeviceTag,
    /// Fields from newer versions.
    pub extra: Extra,
}

impl VaultDoc {
    /// A document with no entries and no identity, to preview an import against before any vault exists. It
    /// is never saved: a real vault gets its own random ids when it is created.
    #[must_use]
    pub fn blank() -> Self {
        Self {
            vault_id: [0; 16],
            entries: Vec::new(),
            tombstones: Vec::new(),
            group: None,
            device: None,
            clock: Hlc {
                wall_ms: 0,
                logical: 0,
                device: DeviceTag([0; 8]),
            },
            device_tag: DeviceTag([0; 8]),
            extra: Extra::new(),
        }
    }
}

#[cfg(test)]
mod tests;

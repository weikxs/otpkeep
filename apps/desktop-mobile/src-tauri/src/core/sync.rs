//! The vault side of pairing and sync (design/06-sync.md §4–§7): what a session needs from the data, and
//! saving what it produced. The sessions themselves run in `crate::sync`.

use keep_sync::group::{
    DeviceRow, GroupError, Staleness, device_rows, found_group, identity_of, leave_group,
    remove_member, rename_self,
};
use keep_sync::identity::{DeviceIdentity, short_fingerprint};
use keep_sync::merge::merge;
use keep_vault::history::HistoryField;
use keep_vault::{DeviceId, EntryId, Platform, VaultDoc};
use serde::Serialize;

use super::{Core, CoreError, Vault};

/// This device and the members of its group, for the sync page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub in_group: bool,
    /// This device's name, short and full fingerprint (only in a group).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<DeviceView>,
    pub devices: Vec<DeviceView>,
    /// The group has its 16 devices.
    pub full: bool,
    /// Whether this device has entries (joining then merges or replaces them).
    pub has_entries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub name: String,
    pub fingerprint: String,
    pub short_fingerprint: String,
    pub platform: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_ms: Option<u64>,
    /// `fresh`, `mayBeBehind` or `considerRemoving`.
    pub staleness: &'static str,
    pub added_by_removed: bool,
    pub this_device: bool,
}

/// What a session starts from.
pub struct SessionStart {
    pub doc: VaultDoc,
    /// This device's identity, when it is in a group.
    pub identity: Option<DeviceIdentity>,
}

#[must_use]
pub const fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "macOS",
        Platform::Windows => "Windows",
        Platform::Linux => "Linux",
        Platform::Ios => "iOS",
        Platform::Android => "Android",
        Platform::Other(_) => "other",
    }
}

fn group_error(error: GroupError) -> CoreError {
    match error {
        GroupError::NotInGroup => CoreError::Invalid("NOT_IN_GROUP"),
        GroupError::BadName => CoreError::Invalid("INVALID_NAME"),
        GroupError::NotAMember => CoreError::NotFound,
        GroupError::Full => CoreError::Invalid("GROUP_FULL"),
        GroupError::AlreadyInGroup => CoreError::Invalid("OTHER_GROUP"),
        GroupError::Removed => CoreError::Invalid("REMOVED"),
        _ => CoreError::Save,
    }
}

impl Core {
    /// This device and its group's members.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NeedsPassword`] before the master password.
    pub fn group_view(&self) -> Result<GroupView, CoreError> {
        let now = (self.clock)();
        let doc = match &self.vault {
            Vault::Fresh => {
                return Ok(GroupView {
                    in_group: false,
                    me: None,
                    devices: Vec::new(),
                    full: false,
                    has_entries: false,
                });
            }
            Vault::Open { service, .. } => service.doc(),
            _ => return Err(CoreError::NeedsPassword),
        };
        let has_entries = doc.entries.iter().any(|e| e.trashed_at.value.is_none());
        if doc.group.is_none() {
            return Ok(GroupView {
                in_group: false,
                me: None,
                devices: Vec::new(),
                full: false,
                has_entries,
            });
        }
        let rows = device_rows(doc, now).map_err(group_error)?;
        let devices: Vec<DeviceView> = rows.iter().map(device_view).collect();
        let full = doc
            .group
            .as_ref()
            .is_some_and(|g| keep_sync::membership::membership(g).is_full());
        Ok(GroupView {
            in_group: true,
            me: devices.iter().find(|d| d.this_device).cloned(),
            devices,
            full,
            has_entries,
        })
    }

    /// The data and identity a session starts from; a first start gets an empty vault first.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the data is not open or read-only.
    pub fn session_start(&mut self) -> Result<SessionStart, CoreError> {
        if matches!(self.vault, Vault::Fresh) {
            self.create()?;
        }
        let service = self.writable()?;
        let doc = service.doc().clone();
        let identity = identity_of(&doc).ok();
        self.session_base = Some(doc.clock);
        Ok(SessionStart { doc, identity })
    }

    /// Founds a sync group with this device named `name`, when it has none.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] for a bad name or when it cannot be saved.
    pub fn ensure_group(&mut self, name: &str) -> Result<(), CoreError> {
        let now = (self.clock)();
        let service = self.writable()?;
        if service.doc().group.is_some() {
            return Ok(());
        }
        let platform = keep_link::device::this_platform();
        let grouped = found_group(service.doc(), platform, name, now).map_err(group_error)?;
        Ok(service.commit_synced(grouped, &[])?)
    }

    /// Saves what a session produced. When the data changed during the session (an edit in the window, a
    /// counter code), the result is merged with the current data once more, so nothing is lost and no HOTP
    /// counter goes back. Whether it changed is told by the data's clock, not the result's: a peer's clock may
    /// put the result ahead of an edit made meanwhile.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when it cannot be saved.
    pub fn commit_session(
        &mut self,
        doc: VaultDoc,
        overwritten: &[(EntryId, HistoryField, String)],
        participants: &[DeviceId],
    ) -> Result<(), CoreError> {
        let base = self.session_base.take();
        let service = self.writable()?;
        let current = service.doc();
        // The result goes first: it carries the group and this device's keys a pairing has just made.
        let doc = if base == Some(current.clock) {
            doc
        } else {
            merge(&doc, current, participants)
        };
        Ok(service.commit_synced(doc, overwritten)?)
    }

    /// Writes an encrypted copy of the current data before pairing replaces it.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Save`] when it cannot be written.
    pub fn backup_before_pairing(&self) -> Result<(), CoreError> {
        let Vault::Open { service, .. } = &self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        service
            .backup_before_pairing()
            .map(drop)
            .map_err(|_| CoreError::Save)
    }

    /// Renames this device in its group; others see it after their next sync.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] with `INVALID_NAME` or `NOT_IN_GROUP`.
    pub fn rename_device(&mut self, name: &str) -> Result<(), CoreError> {
        let now = (self.clock)();
        let service = self.writable()?;
        let renamed = rename_self(service.doc(), name, now).map_err(group_error)?;
        Ok(service.commit_synced(renamed, &[])?)
    }

    /// Removes a member for good; it keeps the data it has.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] for an unknown fingerprint.
    pub fn remove_device(&mut self, fingerprint: &str) -> Result<(), CoreError> {
        let now = (self.clock)();
        let service = self.writable()?;
        let rows = device_rows(service.doc(), now).map_err(group_error)?;
        let row = rows
            .iter()
            .find(|r| r.fingerprint == fingerprint && !r.is_this_device)
            .ok_or(CoreError::NotFound)?;
        let removed = remove_member(service.doc(), row.device, now).map_err(group_error)?;
        Ok(service.commit_synced(removed, &[])?)
    }

    /// Leaves the sync group; the entries stay.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] with `NOT_IN_GROUP`.
    pub fn leave_group(&mut self) -> Result<(), CoreError> {
        let service = self.writable()?;
        let left = leave_group(service.doc()).map_err(group_error)?;
        Ok(service.commit_synced(left, &[])?)
    }
}

fn device_view(row: &DeviceRow) -> DeviceView {
    DeviceView {
        name: row
            .lan_name
            .clone()
            .unwrap_or_else(|| format!("OTPKeep-{}", short_fingerprint(row.device))),
        fingerprint: row.fingerprint.clone(),
        short_fingerprint: short_fingerprint(row.device),
        platform: platform_name(row.platform),
        last_sync_ms: row.last_sync_ms,
        staleness: match row.staleness {
            Staleness::Fresh => "fresh",
            Staleness::MayBeBehind => "mayBeBehind",
            Staleness::ConsiderRemoving => "considerRemoving",
        },
        added_by_removed: row.added_by_removed,
        this_device: row.is_this_device,
    }
}

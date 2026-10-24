use keep_vault::Platform;

use super::{GroupError, admit, found_group, identity_of, join_group};
use crate::identity::{NewDevice, device_tag};
use crate::membership::{MAX_GROUP_MEMBERS, membership};
use crate::testing::{MINUTE_MS, add, device_doc};

fn alone(n: u8) -> keep_vault::VaultDoc {
    let mut doc = device_doc(n);
    doc.group = None;
    doc
}

#[test]
fn founding_a_group_makes_this_device_its_first_member() {
    let (doc, _) = add(&alone(1), "Example Cloud", 1, MINUTE_MS);

    let grouped = found_group(&doc, Platform::MacOs, "Study Mac", 2 * MINUTE_MS).unwrap();

    let identity = identity_of(&grouped).unwrap();
    let members = membership(grouped.group.as_ref().unwrap());
    assert_eq!(members.founder, Some(identity.device_id()));
    assert_eq!(
        members.members[&identity.device_id()].lan_name.as_deref(),
        Some("Study Mac")
    );
    assert_eq!(grouped.device_tag, device_tag(identity.device_id()));
    assert_eq!(grouped.entries, doc.entries, "entries stay as they are");
    assert_eq!(
        found_group(&grouped, Platform::MacOs, "x", 0).err(),
        Some(GroupError::AlreadyInGroup)
    );
}

#[test]
fn a_member_admits_a_new_device_whose_certificate_matches_its_handshake_key() {
    let a = found_group(&alone(1), Platform::MacOs, "Study Mac", MINUTE_MS).unwrap();
    let group_id = a.group.as_ref().unwrap().group_id;
    let joiner = NewDevice::generate().unwrap();
    let handshake_key = x25519_dalek::PublicKey::from(joiner.exchange_secret()).to_bytes();
    let b = joiner.into_identity(&group_id);

    let admitted = admit(
        &a,
        b.device_id(),
        handshake_key,
        &b.dx_cert(),
        Platform::Ios,
        2 * MINUTE_MS,
    )
    .unwrap();

    let members = membership(admitted.group.as_ref().unwrap());
    assert!(members.is_member(b.device_id()));
    assert_eq!(
        admit(&a, b.device_id(), [9; 32], &b.dx_cert(), Platform::Ios, 0).err(),
        Some(GroupError::BadCertificate)
    );
    assert_eq!(
        admit(
            &admitted,
            b.device_id(),
            handshake_key,
            &b.dx_cert(),
            Platform::Ios,
            0
        )
        .err(),
        Some(GroupError::AlreadyMember)
    );
}

#[test]
fn a_full_group_admits_nobody() {
    let mut doc = found_group(&alone(1), Platform::MacOs, "A", MINUTE_MS).unwrap();
    let group_id = doc.group.as_ref().unwrap().group_id;
    for n in 1..MAX_GROUP_MEMBERS {
        let new = NewDevice::generate().unwrap();
        let key = x25519_dalek::PublicKey::from(new.exchange_secret()).to_bytes();
        let id = new.into_identity(&group_id);
        doc = admit(
            &doc,
            id.device_id(),
            key,
            &id.dx_cert(),
            Platform::Linux,
            MINUTE_MS + n as u64,
        )
        .unwrap();
    }
    let new = NewDevice::generate().unwrap();
    let key = x25519_dalek::PublicKey::from(new.exchange_secret()).to_bytes();
    let id = new.into_identity(&group_id);

    assert_eq!(
        admit(&doc, id.device_id(), key, &id.dx_cert(), Platform::Linux, 0).err(),
        Some(GroupError::Full)
    );
}

#[test]
fn a_joining_device_takes_the_group_and_names_itself() {
    let a = found_group(&alone(1), Platform::MacOs, "Study Mac", MINUTE_MS).unwrap();
    let group = a.group.clone().unwrap();
    let (b, _) = add(&alone(2), "Phone entry", 2, MINUTE_MS);
    let new = NewDevice::generate().unwrap();
    let id = new.device_id();
    let identity = new.into_identity(&group.group_id);

    let merged = join_group(&b, &identity, &group, "Phone", false, 2 * MINUTE_MS).unwrap();
    let replaced = join_group(&b, &identity, &group, "Phone", true, 2 * MINUTE_MS).unwrap();

    assert_eq!(merged.entries.len(), 1);
    assert!(replaced.entries.is_empty() && replaced.tombstones.is_empty());
    assert_eq!(merged.device_tag, device_tag(id));
    assert_eq!(identity_of(&merged).unwrap().device_id(), id);
    let renames = merged.group.as_ref().unwrap().events.len() - group.events.len();
    assert_eq!(renames, 1, "its own name, signed by itself");
    assert_eq!(
        join_group(&merged, &identity, &group, "Phone", false, 0).err(),
        Some(GroupError::AlreadyInGroup)
    );
}

mod management {
    use keep_vault::{DeviceId, Hlc, Platform};

    use super::super::{
        DeviceRow, GroupError, Staleness, admit, device_rows, found_group, identity_of,
        leave_group, remove_member, rename_self,
    };
    use crate::identity::NewDevice;
    use crate::membership::membership;
    use crate::testing::{DAY_MS, MINUTE_MS, add, device_doc};

    fn alone(n: u8) -> keep_vault::VaultDoc {
        let mut doc = device_doc(n);
        doc.group = None;
        doc
    }

    /// Device 1's group with a second device added.
    fn two() -> (keep_vault::VaultDoc, DeviceId) {
        let a = found_group(&alone(1), Platform::MacOs, "Study Mac", MINUTE_MS).unwrap();
        let group_id = a.group.as_ref().unwrap().group_id;
        let new = NewDevice::generate().unwrap();
        let key = x25519_dalek::PublicKey::from(new.exchange_secret()).to_bytes();
        let b = new.into_identity(&group_id);
        let a = admit(
            &a,
            b.device_id(),
            key,
            &b.dx_cert(),
            Platform::Ios,
            2 * MINUTE_MS,
        )
        .unwrap();
        (a, b.device_id())
    }

    #[test]
    fn removing_a_member_signs_a_removal() {
        let (a, b) = two();

        let removed = remove_member(&a, b, 3 * MINUTE_MS).unwrap();

        assert!(membership(removed.group.as_ref().unwrap()).is_removed(b));
        assert_eq!(
            remove_member(&removed, b, 4 * MINUTE_MS).err(),
            Some(GroupError::Removed)
        );
        assert_eq!(
            remove_member(&alone(3), b, 0).err(),
            Some(GroupError::NotInGroup)
        );
        assert_eq!(
            remove_member(&a, DeviceId([7; 32]), 0).err(),
            Some(GroupError::NotAMember)
        );
    }

    #[test]
    fn leaving_keeps_the_entries_and_forgets_the_group_and_identity() {
        let (a, _) = two();
        let (a, _) = add(&a, "Kept", 1, 3 * MINUTE_MS);
        let old_tag = a.device_tag;

        let left = leave_group(&a).unwrap();

        assert!(left.group.is_none() && left.device.is_none() && left.tombstones.is_empty());
        assert_eq!(left.entries, a.entries);
        assert_ne!(
            left.device_tag, old_tag,
            "a new device tag: the old identity is gone"
        );
    }

    #[test]
    fn renaming_this_device_signs_its_new_name() {
        let (a, _) = two();
        let me = identity_of(&a).unwrap().device_id();

        let renamed = rename_self(&a, "Office Mac", 3 * MINUTE_MS).unwrap();

        let members = membership(renamed.group.as_ref().unwrap());
        assert_eq!(members.members[&me].lan_name.as_deref(), Some("Office Mac"));
        assert_eq!(rename_self(&a, " ", 0).err(), Some(GroupError::BadName));
        assert_eq!(
            rename_self(&alone(4), "x", 0).err(),
            Some(GroupError::NotInGroup)
        );
    }

    #[test]
    fn the_device_list_shows_last_syncs_and_who_may_be_behind() {
        let (mut a, b) = two();
        let me = identity_of(&a).unwrap().device_id();
        let now = 200 * DAY_MS;
        let seen = |days: u64| Hlc {
            wall_ms: now - days * DAY_MS,
            logical: 0,
            device: keep_vault::DeviceTag([0; 8]),
        };
        let group = a.group.as_mut().unwrap();
        group.presence.insert(b, seen(31));

        let rows = device_rows(&a, now).unwrap();
        let row = |d: DeviceId| rows.iter().find(|r| r.device == d).unwrap().clone();

        assert_eq!(rows.len(), 2);
        assert!(row(me).is_this_device);
        assert_eq!(row(b).staleness, Staleness::MayBeBehind);
        assert_eq!(row(b).last_sync_ms, Some(seen(31).wall_ms));
        assert_eq!(row(b).platform, Platform::Ios);
        let group = a.group.as_mut().unwrap();
        group.presence.insert(b, seen(91));
        assert_eq!(
            row_of(&device_rows(&a, now).unwrap(), b).staleness,
            Staleness::ConsiderRemoving
        );
    }

    fn row_of(rows: &[DeviceRow], device: DeviceId) -> DeviceRow {
        rows.iter().find(|r| r.device == device).unwrap().clone()
    }
}

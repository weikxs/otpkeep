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

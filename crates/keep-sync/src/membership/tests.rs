use std::collections::BTreeMap;

use keep_vault::{DeviceId, DeviceTag, Hlc, MemberEvent, MemberKind, Platform, SyncGroup};

use super::{
    MAX_GROUP_MEMBERS, membership, merge_events, merge_presence, sign_event, verify_event,
};
use crate::identity::DeviceIdentity;

const GROUP: [u8; 16] = [0x42; 16];

fn device(n: u8) -> DeviceIdentity {
    DeviceIdentity::from_seeds(&[n; 32], &[n.wrapping_add(100); 32], &GROUP)
}

fn at(wall_ms: u64) -> Hlc {
    Hlc {
        wall_ms,
        logical: 0,
        device: DeviceTag([0; 8]),
    }
}

fn join(author: &DeviceIdentity, joined: &DeviceIdentity, wall_ms: u64) -> MemberEvent {
    sign_event(
        author,
        &GROUP,
        MemberKind::Join {
            device: joined.device_id(),
            dx_public: joined.dx_public(),
            dx_cert: joined.dx_cert(),
            platform: Platform::MacOs,
        },
        at(wall_ms),
    )
}

fn remove(author: &DeviceIdentity, removed: &DeviceIdentity, wall_ms: u64) -> MemberEvent {
    sign_event(
        author,
        &GROUP,
        MemberKind::Remove {
            device: removed.device_id(),
        },
        at(wall_ms),
    )
}

fn rename(
    author: &DeviceIdentity,
    renamed: &DeviceIdentity,
    name: &str,
    wall_ms: u64,
) -> MemberEvent {
    sign_event(
        author,
        &GROUP,
        MemberKind::Rename {
            device: renamed.device_id(),
            lan_name: name.to_owned(),
        },
        at(wall_ms),
    )
}

fn group(events: Vec<MemberEvent>) -> SyncGroup {
    SyncGroup {
        group_id: GROUP,
        tag_key: [9; 32],
        events,
        presence: BTreeMap::new(),
        extra: BTreeMap::new(),
    }
}

fn members(events: Vec<MemberEvent>) -> Vec<DeviceId> {
    membership(&group(events)).members.keys().copied().collect()
}

fn ids(devices: &[&DeviceIdentity]) -> Vec<DeviceId> {
    let mut ids: Vec<DeviceId> = devices.iter().map(|d| d.device_id()).collect();
    ids.sort();
    ids
}

#[test]
fn the_founder_joins_itself_and_names_itself() {
    let a = device(1);

    let result = membership(&group(vec![
        join(&a, &a, 1),
        rename(&a, &a, "Study Mac", 2),
    ]));

    assert_eq!(result.founder, Some(a.device_id()));
    let member = &result.members[&a.device_id()];
    assert_eq!(member.lan_name.as_deref(), Some("Study Mac"));
    assert_eq!(member.added_by, a.device_id());
    assert_eq!(member.dx_public, a.dx_public());
    assert_eq!(member.platform, Platform::MacOs);
    assert!(!member.added_by_removed);
}

#[test]
fn members_add_members() {
    let (a, b, c) = (device(1), device(2), device(3));

    let result = membership(&group(vec![
        join(&a, &a, 1),
        join(&a, &b, 2),
        join(&b, &c, 3),
    ]));

    assert_eq!(
        result.members.keys().copied().collect::<Vec<_>>(),
        ids(&[&a, &b, &c])
    );
    assert_eq!(result.members[&c.device_id()].added_by, b.device_id());
    assert_eq!(result.members[&c.device_id()].joined_at, at(3));
}

#[test]
fn events_with_bad_signatures_or_certificates_or_outside_authors_count_for_nothing() {
    let (a, b, c, outsider) = (device(1), device(2), device(3), device(9));
    let mut forged = join(&a, &b, 2);
    forged.signature[0] ^= 1;
    let mut wrong_cert = join(&a, &c, 3);
    if let MemberKind::Join { dx_cert, .. } = &mut wrong_cert.kind {
        dx_cert[0] ^= 1;
    }
    let wrong_cert = sign_event(&a, &GROUP, wrong_cert.kind, at(3));

    assert_eq!(
        members(vec![
            join(&a, &a, 1),
            forged,
            wrong_cert,
            join(&outsider, &outsider, 0)
        ]),
        ids(&[&outsider]),
        "the earliest self-join founds the group; nothing else is valid"
    );
    assert_eq!(
        members(vec![join(&a, &a, 1), join(&outsider, &c, 2)]),
        ids(&[&a])
    );
    assert!(!verify_event(
        &GROUP,
        &join(&a, &b, 2).with_group_mismatch()
    ));
}

#[test]
fn only_the_first_self_join_founds_the_group() {
    let (a, b) = (device(1), device(2));

    assert_eq!(members(vec![join(&a, &a, 1), join(&b, &b, 5)]), ids(&[&a]));
}

#[test]
fn removal_is_permanent_and_stops_the_removed_device_from_acting() {
    let (a, b, c, d) = (device(1), device(2), device(3), device(4));
    let events = vec![
        join(&a, &a, 1),
        join(&a, &b, 2),
        join(&b, &c, 3),
        remove(&a, &b, 4),
        join(&b, &d, 5),
        join(&a, &b, 6),
        remove(&b, &a, 7),
    ];

    let result = membership(&group(events));

    assert_eq!(
        result.members.keys().copied().collect::<Vec<_>>(),
        ids(&[&a, &c])
    );
    assert!(result.removed.contains(&b.device_id()));
    assert!(result.is_removed(b.device_id()));
    assert!(!result.is_member(b.device_id()));
    assert!(result.members[&c.device_id()].added_by_removed);
    assert!(!result.members[&a.device_id()].added_by_removed);
}

#[test]
fn any_member_may_remove_any_member_including_itself() {
    let (a, b) = (device(1), device(2));

    assert_eq!(
        members(vec![join(&a, &a, 1), join(&a, &b, 2), remove(&b, &b, 3)]),
        ids(&[&a])
    );
    assert_eq!(
        members(vec![join(&a, &a, 1), join(&a, &b, 2), remove(&b, &a, 3)]),
        ids(&[&b])
    );
}

#[test]
fn a_device_renames_only_itself_and_the_latest_name_wins() {
    let (a, b) = (device(1), device(2));
    let events = vec![
        join(&a, &a, 1),
        join(&a, &b, 2),
        rename(&b, &b, "Phone", 3),
        rename(&b, &b, "Old phone", 2),
        rename(&a, &b, "Hacked", 9),
    ];

    let result = membership(&group(events));

    assert_eq!(
        result.members[&b.device_id()].lan_name.as_deref(),
        Some("Phone")
    );
    assert_eq!(result.members[&a.device_id()].lan_name, None);
}

#[test]
fn kinds_from_newer_versions_are_kept_but_ignored() {
    let a = device(1);
    let other = sign_event(
        &a,
        &GROUP,
        MemberKind::Other {
            kind: 50,
            body: ciborium::Value::Map(vec![(
                ciborium::Value::Integer(0.into()),
                ciborium::Value::Integer(50.into()),
            )]),
        },
        at(3),
    );

    assert!(verify_event(&GROUP, &other));
    assert_eq!(members(vec![join(&a, &a, 1), other.clone()]), ids(&[&a]));
    assert!(merge_events(&GROUP, &[join(&a, &a, 1)], std::slice::from_ref(&other)).contains(&other));
}

#[test]
fn merging_events_is_a_set_union_without_invalid_events() {
    let (a, b, c) = (device(1), device(2), device(3));
    let ours = vec![join(&a, &a, 1), join(&a, &b, 2)];
    let mut forged = join(&a, &c, 3);
    forged.signature[5] ^= 1;
    let theirs = vec![join(&a, &b, 2), join(&b, &c, 4), forged];

    let merged = merge_events(&GROUP, &ours, &theirs);

    assert_eq!(merged.len(), 3);
    assert_eq!(merged, merge_events(&GROUP, &theirs, &ours));
    assert_eq!(merged, merge_events(&GROUP, &merged, &merged));
}

#[test]
fn presence_keeps_the_later_time_per_device() {
    let (a, b, c) = (DeviceId([1; 32]), DeviceId([2; 32]), DeviceId([3; 32]));
    let ours = BTreeMap::from([(a, at(5)), (b, at(1))]);
    let theirs = BTreeMap::from([(b, at(4)), (c, at(2))]);

    let merged = merge_presence(&ours, &theirs);

    assert_eq!(merged, BTreeMap::from([(a, at(5)), (b, at(4)), (c, at(2))]));
    assert_eq!(merged, merge_presence(&theirs, &ours));
}

#[test]
fn a_group_is_full_at_sixteen_members() {
    let founder = device(1);
    let mut events = vec![join(&founder, &founder, 1)];
    for n in 2..=u8::try_from(MAX_GROUP_MEMBERS).unwrap() {
        events.push(join(&founder, &device(n), u64::from(n)));
        let result = membership(&group(events.clone()));
        assert_eq!(result.is_full(), usize::from(n) == MAX_GROUP_MEMBERS, "{n}");
    }
}

#[test]
fn the_order_events_arrive_in_does_not_matter() {
    let (a, b, c) = (device(1), device(2), device(3));
    let events = vec![
        join(&a, &a, 1),
        join(&a, &b, 2),
        join(&b, &c, 3),
        remove(&a, &b, 4),
        rename(&c, &c, "Tablet", 5),
    ];
    let mut reversed = events.clone();
    reversed.reverse();

    assert_eq!(membership(&group(events)), membership(&group(reversed)));
}

trait GroupMismatch {
    fn with_group_mismatch(self) -> Self;
}

impl GroupMismatch for MemberEvent {
    /// The same event signed for another group.
    fn with_group_mismatch(self) -> Self {
        let mut other = [0x43; 16];
        other[0] = 1;
        let a = device(1);
        sign_event(&a, &other, self.kind, self.stamp)
    }
}

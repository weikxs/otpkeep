use keep_vault::edit::{Change, edit_entry};
use keep_vault::history::HistoryField;
use keep_vault::{MemberKind, VaultDoc};

use super::{SyncStop, sync_data};
use crate::digest::state_digest;
use crate::membership::sign_event;
use crate::testing::{DAY_MS, GROUP, MINUTE_MS, add, entry, founded, identity, member_doc};

fn pair() -> (VaultDoc, VaultDoc) {
    let events = founded(1, &[2, 3]);
    (member_doc(1, &events), member_doc(2, &events))
}

fn ids() -> (keep_vault::DeviceId, keep_vault::DeviceId) {
    (identity(1).device_id(), identity(2).device_id())
}

#[test]
fn both_devices_end_with_the_same_data_and_record_the_sync() {
    let (a, b) = pair();
    let (a, first) = add(&a, "Example Cloud", 1, MINUTE_MS);
    let (b, second) = add(&b, "Demo Git", 2, MINUTE_MS);
    let (a_id, b_id) = ids();

    let done = sync_data(&a, &b, a_id, b_id, 2 * MINUTE_MS, 2 * MINUTE_MS).unwrap();

    assert_eq!(state_digest(&done.opener), state_digest(&done.joiner));
    assert_eq!(done.digest, state_digest(&done.opener));
    for doc in [&done.opener, &done.joiner] {
        assert!(doc.entries.iter().any(|e| e.id == first));
        assert!(doc.entries.iter().any(|e| e.id == second));
        let presence = &doc.group.as_ref().unwrap().presence;
        assert!(presence.contains_key(&a_id) && presence.contains_key(&b_id));
    }
    assert_eq!(done.opener.vault_id, a.vault_id);
    assert_eq!(done.joiner.vault_id, b.vault_id);
}

#[test]
fn the_opener_removes_duplicates_and_the_joiner_gets_the_result() {
    let (a, b) = pair();
    let (a, _) = add(&a, "Mail", 1, MINUTE_MS);
    let (b, second) = add(&b, "Mail", 1, 2 * MINUTE_MS);
    let (a_id, b_id) = ids();

    let done = sync_data(&a, &b, a_id, b_id, 3 * MINUTE_MS, 3 * MINUTE_MS).unwrap();

    assert_eq!(done.deduped, vec![second]);
    assert!(entry(&done.joiner, second).trashed_at.value.is_some());
}

#[test]
fn names_and_notes_overwritten_on_each_side_are_reported_for_its_history() {
    let (a, b) = pair();
    let (a, id) = add(&a, "Old", 1, MINUTE_MS);
    let (a_id, b_id) = ids();
    let first = sync_data(&a, &b, a_id, b_id, MINUTE_MS, MINUTE_MS).unwrap();
    let b = edit_entry(
        &first.joiner,
        id,
        Change::Title("New".into()),
        2 * MINUTE_MS,
    )
    .unwrap();

    let done = sync_data(&first.opener, &b, a_id, b_id, 3 * MINUTE_MS, 3 * MINUTE_MS).unwrap();

    assert_eq!(
        done.opener_overwritten,
        vec![(id, HistoryField::Title, "Old".to_owned())]
    );
    assert!(done.joiner_overwritten.is_empty());
}

#[test]
fn a_removed_device_cannot_sync_with_anyone_who_knows() {
    let (a, b) = pair();
    let removal = sign_event(
        &identity(1),
        &GROUP,
        MemberKind::Remove {
            device: identity(2).device_id(),
        },
        keep_vault::Hlc {
            wall_ms: 10,
            logical: 0,
            device: a.device_tag,
        },
    );
    let mut a = a;
    a.group.as_mut().unwrap().events.push(removal);
    let (a_id, b_id) = ids();

    assert_eq!(
        sync_data(&a, &b, a_id, b_id, MINUTE_MS, MINUTE_MS).err(),
        Some(SyncStop::Removed(b_id))
    );
    assert_eq!(
        sync_data(&b, &a, b_id, a_id, MINUTE_MS, MINUTE_MS).err(),
        Some(SyncStop::Removed(b_id))
    );
}

#[test]
fn devices_outside_the_group_are_refused() {
    let (a, _) = pair();
    let mut stranger = member_doc(9, &founded(9, &[]));
    stranger.group.as_mut().unwrap().group_id = [0x43; 16];
    let (a_id, _) = ids();

    assert_eq!(
        sync_data(
            &a,
            &stranger,
            a_id,
            identity(9).device_id(),
            MINUTE_MS,
            MINUTE_MS
        )
        .err(),
        Some(SyncStop::OtherGroup)
    );
    let (b_in_other_role, _) = pair();
    assert_eq!(
        sync_data(
            &a,
            &b_in_other_role,
            a_id,
            identity(5).device_id(),
            MINUTE_MS,
            MINUTE_MS
        )
        .err(),
        Some(SyncStop::NotInGroup(identity(5).device_id()))
    );
}

#[test]
fn a_peer_clock_far_ahead_stops_the_session_without_changes() {
    let (a, b) = pair();
    let (b, _) = add(&b, "Future", 1, 3 * DAY_MS);
    let (a_id, b_id) = ids();

    let stop = sync_data(&a, &b, a_id, b_id, DAY_MS, 3 * DAY_MS).err();

    assert!(matches!(stop, Some(SyncStop::PeerClock(_))), "{stop:?}");
}

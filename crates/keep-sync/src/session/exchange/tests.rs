use keep_vault::edit::{Change, NewEntry, add_entry_with_id, edit_entry};
use keep_vault::{EntryId, MemberKind, VaultDoc};

use super::{Exchanged, SessionError, exchange_as_joiner, exchange_as_opener};
use crate::channel::message::ErrorCode;
use crate::digest::state_digest;
use crate::membership::{membership, sign_event};
use crate::session::sync_data;
use crate::testing::{
    DAY_MS, GROUP, MINUTE_MS, add, channel_pair, founded, identity, member_doc, new_entry,
};

fn pair() -> (VaultDoc, VaultDoc) {
    let events = founded(1, &[2, 3]);
    (member_doc(1, &events), member_doc(2, &events))
}

async fn exchange(
    a: &VaultDoc,
    b: &VaultDoc,
    a_now: u64,
    b_now: u64,
) -> (
    Result<Exchanged, SessionError>,
    Result<Exchanged, SessionError>,
) {
    let (mut opener, mut joiner) = channel_pair().await;
    let (a_id, b_id) = (identity(1).device_id(), identity(2).device_id());
    tokio::join!(
        exchange_as_opener(&mut opener, a, a_id, b_id, a_now),
        exchange_as_joiner(&mut joiner, b, b_id, a_id, b_now)
    )
}

#[tokio::test]
async fn both_sides_end_with_what_the_in_memory_merge_computes() {
    let (a, b) = pair();
    let (a, shared) = add(&a, "Example Cloud", 1, MINUTE_MS);
    let b = crate::merge::merge(&b, &a, &[identity(1).device_id(), identity(2).device_id()]);
    let a = edit_entry(&a, shared, Change::Title("Cloud (A)".into()), 2 * MINUTE_MS).unwrap();
    let b = edit_entry(
        &b,
        shared,
        Change::Note("note from B".into()),
        3 * MINUTE_MS,
    )
    .unwrap();
    let (a, _) = add(&a, "Only A", 2, 4 * MINUTE_MS);
    let (b, _) = add(&b, "Only B", 3, 4 * MINUTE_MS);

    let (on_a, on_b) = exchange(&a, &b, 5 * MINUTE_MS, 5 * MINUTE_MS).await;
    let (on_a, on_b) = (on_a.unwrap(), on_b.unwrap());

    let expected = sync_data(
        &a,
        &b,
        identity(1).device_id(),
        identity(2).device_id(),
        5 * MINUTE_MS,
        5 * MINUTE_MS,
    )
    .unwrap();
    assert_eq!(on_a.digest, on_b.digest);
    assert_eq!(on_a.digest, expected.digest);
    assert_eq!(state_digest(&on_a.doc), on_a.digest);
    assert_eq!(state_digest(&on_b.doc), on_b.digest);
    assert_eq!(on_a.doc.vault_id, a.vault_id, "local parts stay");
    assert_eq!(on_b.doc.vault_id, b.vault_id);
    assert!(on_a.doc.device == a.device && on_b.doc.device == b.device);
}

#[tokio::test]
async fn the_opener_removes_duplicates_and_the_joiner_receives_the_change() {
    let (a, b) = pair();
    let (a, _) = add(&a, "Mail", 1, MINUTE_MS);
    let (b, second) = add(&b, "Mail", 1, 2 * MINUTE_MS);

    let (on_a, on_b) = exchange(&a, &b, 3 * MINUTE_MS, 3 * MINUTE_MS).await;
    let (on_a, on_b) = (on_a.unwrap(), on_b.unwrap());

    assert_eq!(on_a.deduped, vec![second]);
    assert_eq!(on_a.digest, on_b.digest);
    let moved = on_b.doc.entries.iter().find(|e| e.id == second).unwrap();
    assert!(moved.trashed_at.value.is_some());
}

#[tokio::test]
async fn thousands_of_entries_travel_in_batches() {
    let (a, b) = pair();
    let template = NewEntry {
        note: "x".repeat(400),
        ..new_entry("Account", 0x33)
    };
    let (with_one, _) = add_entry_with_id(&a, template, EntryId([0; 16]), MINUTE_MS).unwrap();
    let one = with_one.entries[0].clone();
    let entries = (0..3_000u32)
        .map(|n| {
            let mut entry = one.clone();
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&n.to_be_bytes());
            entry.id = EntryId(id);
            let mut secret = vec![0x33; 20];
            secret[..4].copy_from_slice(&n.to_be_bytes());
            entry.key.value.secret = keep_passcode::SecretKey::from_bytes(secret).unwrap();
            entry
        })
        .collect();
    let a = VaultDoc {
        entries,
        ..with_one
    };

    let (on_a, on_b) = exchange(&a, &b, 2 * MINUTE_MS, 2 * MINUTE_MS).await;
    let (on_a, on_b) = (on_a.unwrap(), on_b.unwrap());

    assert_eq!(on_b.doc.entries.len(), 3_000);
    assert_eq!(on_a.digest, on_b.digest);
}

#[tokio::test]
async fn a_removal_stops_the_session_and_both_keep_the_membership_events() {
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
    let (b, _) = add(&b, "Not shared", 1, MINUTE_MS);

    let (on_a, on_b) = exchange(&a, &b, 2 * MINUTE_MS, 2 * MINUTE_MS).await;

    for (side, original) in [(on_a, &a), (on_b, &b)] {
        let Err(SessionError::Removed { keep }) = side else {
            panic!("expected the session to stop over the removal");
        };
        assert!(membership(keep.group.as_ref().unwrap()).is_removed(identity(2).device_id()));
        assert_eq!(
            keep.entries, original.entries,
            "only membership events are kept"
        );
    }
}

#[tokio::test]
async fn a_clock_far_ahead_stops_the_session_on_both_sides() {
    let (a, b) = pair();
    let (b, _) = add(&b, "From the future", 1, 3 * DAY_MS);

    let (on_a, on_b) = exchange(&a, &b, DAY_MS, 3 * DAY_MS).await;

    assert!(
        matches!(on_a, Err(SessionError::Stopped(ErrorCode::PeerClock))),
        "{on_a:?}"
    );
    assert!(
        matches!(on_b, Err(SessionError::Stopped(ErrorCode::PeerClock))),
        "{on_b:?}"
    );
}

use std::collections::BTreeSet;

use keep_vault::edit::{NewEntry, add_entry, move_to_trash, purge};
use keep_vault::{EntryId, VaultDoc};

use super::{TOMBSTONE_GRACE_MS, dedupe, prune_tombstones};
use crate::merge::merge;
use crate::testing::{DAY_MS, MINUTE_MS, add, device_doc, device_id, entry, new_entry, on_device};

/// The same account added separately on two devices, then merged.
fn added_twice() -> (VaultDoc, EntryId, EntryId) {
    let (a, first) = add(&device_doc(1), "Mail", 1, MINUTE_MS);
    let (b, second) = add(&device_doc(2), "Mail (phone)", 1, 2 * MINUTE_MS);
    (merge(&a, &b, &[device_id(1), device_id(2)]), first, second)
}

#[test]
fn the_older_of_two_entries_with_one_key_stays_and_the_other_goes_to_the_trash() {
    let (doc, first, second) = added_twice();

    let (deduped, trashed) = dedupe(&doc, 3 * MINUTE_MS);

    assert_eq!(trashed, vec![second]);
    assert_eq!(entry(&deduped, first).trashed_at.value, None);
    let moved = entry(&deduped, second);
    assert!(moved.trashed_at.value.is_some());
    assert_eq!(moved.title.value, "Mail (phone)", "nothing else changes");
}

#[test]
fn entries_the_user_kept_as_duplicates_are_left_alone() {
    let (a, _) = add(&device_doc(1), "Mail", 1, MINUTE_MS);
    let (a, _) = add_entry(
        &a,
        NewEntry {
            keep_duplicate: true,
            ..new_entry("Mail again", 1)
        },
        2 * MINUTE_MS,
    )
    .unwrap();

    let (deduped, trashed) = dedupe(&a, 3 * MINUTE_MS);

    assert!(trashed.is_empty());
    assert_eq!(deduped, a);
}

#[test]
fn entries_already_in_the_trash_do_not_count() {
    let (doc, first, second) = added_twice();
    let doc = move_to_trash(&doc, first, 3 * MINUTE_MS).unwrap();

    let (_, trashed) = dedupe(&doc, 4 * MINUTE_MS);

    assert!(
        trashed.is_empty(),
        "{second:?} is the only one left outside the trash"
    );
}

#[test]
fn every_device_picks_the_same_entry_to_keep() {
    let (doc, _, second) = added_twice();
    let mut reversed = on_device(&doc, 2);
    reversed.entries.reverse();

    assert_eq!(dedupe(&doc, 3 * MINUTE_MS).1, vec![second]);
    assert_eq!(dedupe(&reversed, 3 * MINUTE_MS).1, vec![second]);
}

fn deleted(now: u64) -> VaultDoc {
    let (doc, id) = add(&device_doc(1), "Old", 1, MINUTE_MS);
    purge(&doc, id, now).unwrap()
}

fn members(ns: &[u8]) -> BTreeSet<keep_vault::DeviceId> {
    ns.iter().map(|n| device_id(*n)).collect()
}

#[test]
fn a_tombstone_is_cleared_once_every_member_holds_it_and_the_grace_period_passed() {
    let doc = merge(
        &deleted(DAY_MS),
        &device_doc(2),
        &[device_id(1), device_id(2)],
    );
    let late = DAY_MS + TOMBSTONE_GRACE_MS + 1;

    assert_eq!(
        prune_tombstones(&doc, &members(&[1, 2]), late)
            .tombstones
            .len(),
        0
    );
    assert_eq!(
        prune_tombstones(&doc, &members(&[1, 2]), DAY_MS + TOMBSTONE_GRACE_MS)
            .tombstones
            .len(),
        1,
        "not before the grace period"
    );
    assert_eq!(
        prune_tombstones(&doc, &members(&[1, 2, 3]), late)
            .tombstones
            .len(),
        1,
        "not while a member may still hold the entry"
    );
    assert_eq!(
        prune_tombstones(&doc, &members(&[1]), late)
            .tombstones
            .len(),
        0,
        "devices no longer in the group do not count"
    );
}

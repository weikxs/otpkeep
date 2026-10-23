use ciborium::Value;
use keep_vault::edit::{Change, edit_entry, move_to_trash, next_hotp, purge, replace_key, restore};
use keep_vault::{Entry, EntryId, VaultDoc};

use super::merge;
use crate::digest::state_digest;
use crate::testing::{
    DAY_MS, MINUTE_MS, add, device_doc, device_id, entry, hotp, new_entry, on_device, secret,
};

const PARTICIPANTS: [keep_vault::DeviceId; 2] =
    [keep_vault::DeviceId([1; 32]), keep_vault::DeviceId([2; 32])];

/// Device 1 and device 2 holding the same entry, added on device 1.
fn shared() -> (VaultDoc, VaultDoc, EntryId) {
    let (a, id) = add(&device_doc(1), "Example Cloud", 1, MINUTE_MS);
    (a.clone(), on_device(&a, 2), id)
}

/// Merges both ways and checks that the two devices end up with the same synced data.
fn sync(a: &VaultDoc, b: &VaultDoc) -> VaultDoc {
    let on_a = merge(a, b, &PARTICIPANTS);
    let on_b = merge(b, a, &PARTICIPANTS);
    assert_eq!(
        state_digest(&on_a),
        state_digest(&on_b),
        "merge must not depend on the side"
    );
    on_a
}

fn in_trash(doc: &VaultDoc) -> Vec<&Entry> {
    doc.entries
        .iter()
        .filter(|e| e.trashed_at.value.is_some())
        .collect()
}

#[test]
fn the_later_edit_of_each_field_wins() {
    let (a, b, id) = shared();
    let a = edit_entry(&a, id, Change::Title("Cloud (work)".into()), 2 * MINUTE_MS).unwrap();
    let a = edit_entry(&a, id, Change::Note("old note".into()), 2 * MINUTE_MS).unwrap();
    let b = edit_entry(&b, id, Change::Note("new note".into()), 3 * MINUTE_MS).unwrap();
    let b = edit_entry(&b, id, Change::Pinned(true), 3 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    let e = entry(&merged, id);
    assert_eq!(e.title.value, "Cloud (work)");
    assert_eq!(e.note.value, "new note");
    assert!(e.pinned.value);
}

#[test]
fn entries_on_one_side_are_added() {
    let (a, b, first) = shared();
    let (a, second) = add(&a, "Demo Git", 2, 2 * MINUTE_MS);
    let (b, third) = add(&b, "Demo Mail", 3, 2 * MINUTE_MS);

    let merged = sync(&a, &b);

    for id in [first, second, third] {
        assert!(merged.entries.iter().any(|e| e.id == id));
    }
    assert_eq!(merged.entries.len(), 3);
}

#[test]
fn hotp_counters_of_the_same_key_take_the_higher_value() {
    let (a, id) = keep_vault::edit::add_entry(
        &device_doc(1),
        keep_vault::edit::NewEntry {
            spec: hotp(),
            ..new_entry("Counter", 4)
        },
        MINUTE_MS,
    )
    .unwrap();
    let b = on_device(&a, 2);
    let (a, _) = next_hotp(&a, id, 2 * MINUTE_MS).unwrap();
    let (b, _) = next_hotp(&b, id, 2 * MINUTE_MS).unwrap();
    let (b, _) = next_hotp(&b, id, 2 * MINUTE_MS).unwrap();
    let (a, _) = next_hotp(&a, id, 3 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert_eq!(entry(&merged, id).key.value.counter, 2);
    assert!(
        in_trash(&merged).is_empty(),
        "the same key never makes a copy"
    );
}

#[test]
fn a_replaced_key_wins_and_the_old_one_is_kept_in_the_trash() {
    let (a, id) = keep_vault::edit::add_entry(
        &device_doc(1),
        keep_vault::edit::NewEntry {
            spec: hotp(),
            ..new_entry("Counter", 4)
        },
        MINUTE_MS,
    )
    .unwrap();
    let b = on_device(&a, 2);
    let a = replace_key(&a, id, secret(5), hotp(), 0, 2 * MINUTE_MS).unwrap();
    let (b, _) = next_hotp(&b, id, 3 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert_eq!(entry(&merged, id).key.value.secret, secret(5));
    let copies = in_trash(&merged);
    let [copy] = copies.as_slice() else {
        panic!("expected one old-key copy, got {}", copies.len());
    };
    assert_eq!(copy.key.value.secret, secret(4));
    assert_eq!(
        copy.key.value.counter, 1,
        "the step taken on the other device is kept"
    );
    assert_eq!(copy.old_key_of.value, Some(id));
    assert_eq!(
        sync(&merged, &b).entries.len(),
        2,
        "syncing again adds no more copies"
    );
}

#[test]
fn of_two_replacements_the_later_wins_and_neither_key_is_lost() {
    let (a, b, id) = shared();
    let a = replace_key(
        &a,
        id,
        secret(5),
        keep_passcode::CodeSpec::default(),
        0,
        2 * MINUTE_MS,
    )
    .unwrap();
    let b = replace_key(
        &b,
        id,
        secret(6),
        keep_passcode::CodeSpec::default(),
        0,
        3 * MINUTE_MS,
    )
    .unwrap();

    let merged = sync(&a, &b);

    assert_eq!(entry(&merged, id).key.value.secret, secret(6));
    let mut kept: Vec<Vec<u8>> = in_trash(&merged)
        .iter()
        .map(|e| e.key.value.secret.expose().to_vec())
        .collect();
    kept.sort();
    assert_eq!(kept, vec![vec![1; 20], vec![5; 20]]);
}

#[test]
fn an_entry_edited_on_one_device_and_trashed_on_the_other_is_in_the_trash_with_the_edit() {
    let (a, b, id) = shared();
    let a = edit_entry(&a, id, Change::Note("still needed".into()), 2 * MINUTE_MS).unwrap();
    let b = move_to_trash(&b, id, 3 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    let e = entry(&merged, id);
    assert!(e.trashed_at.value.is_some());
    assert_eq!(e.note.value, "still needed");
}

#[test]
fn a_tombstone_deletes_an_entry_not_changed_since() {
    let (a, b, id) = shared();
    let b = move_to_trash(&b, id, 2 * MINUTE_MS).unwrap();
    let b = purge(&b, id, 3 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert!(merged.entries.is_empty());
    assert_eq!(merged.tombstones.len(), 1);
    assert_eq!(merged.tombstones[0].acked_by, vec![[1; 32], [2; 32]]);
}

#[test]
fn an_entry_changed_after_its_deletion_survives_in_the_trash() {
    let (a, b, id) = shared();
    let b = move_to_trash(&b, id, 2 * MINUTE_MS).unwrap();
    let b = purge(&b, id, 3 * MINUTE_MS).unwrap();
    let a = edit_entry(&a, id, Change::Note("edited later".into()), 4 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert!(merged.tombstones.is_empty());
    let e = entry(&merged, id);
    assert_eq!(e.note.value, "edited later");
    assert!(e.trashed_at.value.is_some(), "it comes back in the trash");
}

#[test]
fn a_restore_after_the_deletion_time_keeps_the_entry_out_of_the_trash() {
    let (a, b, id) = shared();
    let b = move_to_trash(&b, id, 2 * MINUTE_MS).unwrap();
    let b = purge(&b, id, 3 * MINUTE_MS).unwrap();
    let a = move_to_trash(&a, id, 2 * MINUTE_MS).unwrap();
    let a = restore(&a, id, 4 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert_eq!(entry(&merged, id).trashed_at.value, None);
}

#[test]
fn tombstones_merge_as_a_set_with_everyone_who_holds_them() {
    let (a, b, id) = shared();
    let a = purge(&a, id, 2 * MINUTE_MS).unwrap();
    let b = purge(&b, id, 3 * MINUTE_MS).unwrap();
    let mut b = b;
    b.tombstones[0].acked_by = vec![device_id(7).0];

    let merged = sync(&a, &b);

    let [tomb] = merged.tombstones.as_slice() else {
        panic!("expected one tombstone");
    };
    assert_eq!(tomb.stamp, b.tombstones[0].stamp, "the later deletion time");
    assert_eq!(tomb.acked_by, vec![[1; 32], [2; 32], [7; 32]]);
}

#[test]
fn fields_from_newer_versions_are_kept() {
    let (mut a, b, id) = shared();
    a.entries[0].extra.insert(40, Value::Text("future".into()));
    let a = edit_entry(&a, id, Change::Note("newer".into()), 2 * MINUTE_MS).unwrap();

    let merged = sync(&a, &b);

    assert_eq!(
        entry(&merged, id).extra.get(&40),
        Some(&Value::Text("future".into()))
    );
}

#[test]
fn local_parts_stay_and_the_clock_moves_past_everything_seen() {
    let (a, b, id) = shared();
    let b = edit_entry(&b, id, Change::Title("later".into()), DAY_MS).unwrap();

    let merged = merge(&a, &b, &PARTICIPANTS);

    assert_eq!(merged.vault_id, a.vault_id);
    assert_eq!(merged.device_tag, a.device_tag);
    assert!(merged.clock >= b.clock);
}

#[test]
fn merging_is_idempotent() {
    let (a, b, id) = shared();
    let a = edit_entry(&a, id, Change::Title("A".into()), 2 * MINUTE_MS).unwrap();
    let merged = sync(&a, &b);

    assert_eq!(
        state_digest(&merge(&merged, &merged, &PARTICIPANTS)),
        state_digest(&merged)
    );
    assert_eq!(state_digest(&sync(&merged, &a)), state_digest(&merged));
}

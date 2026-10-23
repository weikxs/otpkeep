use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use proptest::prelude::*;

use super::{
    Change, EditError, NOTE_MAX_CHARS, NewEntry, Placement, TAG_MAX_CHARS, TAGS_MAX,
    TITLE_MAX_CHARS, TRASH_RETENTION_MS, add_entry, edit_entry, find_duplicate, move_entry,
    move_to_trash, next_hotp, ordered, purge, purge_expired_trash, replace_key, restore, trashed,
};
use crate::{DeviceTag, EntryId, Extra, Hlc, VaultDoc};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

fn empty() -> VaultDoc {
    VaultDoc {
        vault_id: [0; 16],
        entries: vec![],
        tombstones: vec![],
        group: None,
        device: None,
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([7; 8]),
        extra: Extra::new(),
    }
}

fn secret(byte: u8) -> SecretKey {
    SecretKey::from_bytes(vec![byte; 20]).unwrap()
}

fn new_entry(title: &str, byte: u8) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "alex@example.com".to_owned(),
        secret: secret(byte),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: vec![],
        keep_duplicate: false,
    }
}

fn hotp() -> CodeSpec {
    CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap()
}

fn add(doc: &VaultDoc, title: &str, byte: u8, now: u64) -> (VaultDoc, EntryId) {
    add_entry(doc, new_entry(title, byte), now).unwrap()
}

fn titles(doc: &VaultDoc) -> Vec<String> {
    ordered(doc).iter().map(|e| e.title.value.clone()).collect()
}

fn entry(doc: &VaultDoc, id: EntryId) -> &crate::Entry {
    doc.entries.iter().find(|e| e.id == id).unwrap()
}

#[test]
fn adds_an_entry_with_one_stamp_on_every_field() {
    let (doc, id) = add(&empty(), "  Cloud (work)  ", 1, 1_000);
    let e = entry(&doc, id);

    assert_eq!(e.title.value, "Cloud (work)");
    assert_eq!(e.issuer.value, "Example Cloud");
    assert_eq!(e.key.value.secret, secret(1));
    assert_eq!(e.key.value.key_id, e.created_at);
    assert_eq!(e.trashed_at.value, None);
    assert_eq!(e.old_key_of.value, None);
    for stamp in [
        e.title.stamp,
        e.note.stamp,
        e.key.stamp,
        e.rank.stamp,
        e.trashed_at.stamp,
    ] {
        assert_eq!(stamp, e.created_at);
    }
    assert_eq!(e.created_at.device, DeviceTag([7; 8]));
    assert_eq!(doc.clock, e.created_at);
}

#[test]
fn new_entries_go_to_the_end() {
    let (doc, _) = add(&empty(), "first", 1, 1);
    let (doc, _) = add(&doc, "second", 2, 2);
    let (doc, _) = add(&doc, "third", 3, 3);

    assert_eq!(titles(&doc), ["first", "second", "third"]);
}

#[test]
fn stamps_keep_increasing_even_when_the_clock_goes_back() {
    let (doc, a) = add(&empty(), "a", 1, 5_000);
    let (doc, b) = add(&doc, "b", 2, 4_000);
    let (doc, c) = add(&doc, "c", 3, 4_000);

    let (sa, sb, sc) = (
        entry(&doc, a).created_at,
        entry(&doc, b).created_at,
        entry(&doc, c).created_at,
    );
    assert!(sa < sb && sb < sc, "{sa:?} {sb:?} {sc:?}");
    assert_eq!(sb.wall_ms, 5_000);
}

#[test]
fn validates_lengths_in_characters() {
    let long_title: String = "支".repeat(TITLE_MAX_CHARS + 1);
    let max_title: String = "支".repeat(TITLE_MAX_CHARS);
    let long_note = "n".repeat(NOTE_MAX_CHARS + 1);

    assert_eq!(
        add_entry(&empty(), new_entry("   ", 1), 1).err(),
        Some(EditError::TitleEmpty)
    );
    assert_eq!(
        add_entry(&empty(), new_entry(&long_title, 1), 1).err(),
        Some(EditError::TitleTooLong)
    );
    assert!(add_entry(&empty(), new_entry(&max_title, 1), 1).is_ok());
    let with_note = NewEntry {
        note: long_note,
        ..new_entry("t", 1)
    };
    assert_eq!(
        add_entry(&empty(), with_note, 1).err(),
        Some(EditError::NoteTooLong)
    );
    let long_issuer = NewEntry {
        issuer: "i".repeat(257),
        ..new_entry("t", 1)
    };
    assert_eq!(
        add_entry(&empty(), long_issuer, 1).err(),
        Some(EditError::IssuerTooLong)
    );
}

#[test]
fn tags_are_trimmed_deduplicated_and_limited() {
    let tagged = NewEntry {
        tags: vec![
            " Work ".into(),
            "work".into(),
            String::new(),
            "云".into(),
            "WORK".into(),
        ],
        ..new_entry("t", 1)
    };
    let (doc, id) = add_entry(&empty(), tagged, 1).unwrap();

    assert_eq!(entry(&doc, id).tags.value, ["Work", "云"]);
    let too_many = NewEntry {
        tags: (0..=TAGS_MAX).map(|i| format!("t{i}")).collect(),
        ..new_entry("t", 1)
    };
    assert_eq!(
        add_entry(&empty(), too_many, 1).err(),
        Some(EditError::TooManyTags)
    );
    let too_long = NewEntry {
        tags: vec!["x".repeat(TAG_MAX_CHARS + 1)],
        ..new_entry("t", 1)
    };
    assert_eq!(
        add_entry(&empty(), too_long, 1).err(),
        Some(EditError::TagTooLong)
    );
}

#[test]
fn a_secret_already_present_is_a_duplicate_even_with_other_parameters_or_in_the_trash() {
    let (doc, existing) = add(&empty(), "a", 1, 1);
    let other_params = NewEntry {
        spec: hotp(),
        ..new_entry("b", 1)
    };

    assert_eq!(
        add_entry(&doc, other_params, 2).err(),
        Some(EditError::Duplicate { existing })
    );
    let doc = move_to_trash(&doc, existing, 3).unwrap();
    assert_eq!(find_duplicate(&doc, &secret(1)), Some(existing));
    assert_eq!(
        add_entry(&doc, new_entry("c", 1), 4).err(),
        Some(EditError::Duplicate { existing })
    );
}

#[test]
fn add_anyway_keeps_the_duplicate_and_marks_it() {
    let (doc, _) = add(&empty(), "a", 1, 1);
    let anyway = NewEntry {
        keep_duplicate: true,
        ..new_entry("b", 1)
    };

    let (doc, id) = add_entry(&doc, anyway, 2).unwrap();

    assert!(entry(&doc, id).keep_duplicate.value);
    assert_eq!(doc.entries.len(), 2);
}

#[test]
fn edits_change_one_field_and_its_stamp_only() {
    let (doc, id) = add(&empty(), "a", 1, 1);
    let before = entry(&doc, id).clone();

    let doc = edit_entry(&doc, id, Change::Note("备注\n第二行".into()), 10).unwrap();
    let doc = edit_entry(&doc, id, Change::Tags(vec!["x".into(), "X".into()]), 11).unwrap();
    let doc = edit_entry(&doc, id, Change::Pinned(true), 12).unwrap();
    let after = entry(&doc, id);

    assert_eq!(after.note.value, "备注\n第二行");
    assert!(after.note.stamp > before.note.stamp);
    assert_eq!(after.tags.value, ["x"]);
    assert!(after.pinned.value);
    assert_eq!(after.title, before.title);
    assert_eq!(after.key, before.key);
}

#[test]
fn edits_are_validated_like_new_entries() {
    let (doc, id) = add(&empty(), "a", 1, 1);

    assert_eq!(
        edit_entry(&doc, id, Change::Title(" ".into()), 2).err(),
        Some(EditError::TitleEmpty)
    );
    assert_eq!(
        edit_entry(&doc, id, Change::Account("a".repeat(257)), 2).err(),
        Some(EditError::AccountTooLong)
    );
    assert_eq!(
        edit_entry(&doc, EntryId([9; 16]), Change::Pinned(true), 2).err(),
        Some(EditError::NotFound)
    );
}

#[test]
fn trash_restore_and_purge() {
    let (doc, a) = add(&empty(), "a", 1, 1);
    let (doc, b) = add(&doc, "b", 2, 2);

    let doc = move_to_trash(&doc, a, 3).unwrap();
    assert_eq!(titles(&doc), ["b"]);
    assert_eq!(trashed(&doc).iter().map(|e| e.id).collect::<Vec<_>>(), [a]);
    let doc = restore(&doc, a, 4).unwrap();
    assert_eq!(titles(&doc), ["a", "b"]);
    let doc = purge(&doc, b, DAY_MS).unwrap();
    assert_eq!(doc.entries.len(), 1);
    assert_eq!(purge(&doc, b, DAY_MS).err(), Some(EditError::NotFound));
}

#[test]
fn the_trash_empties_itself_after_thirty_days() {
    let (doc, old) = add(&empty(), "old", 1, 1);
    let (doc, recent) = add(&doc, "recent", 2, 2);
    let (doc, kept) = add(&doc, "kept", 3, 3);
    let doc = move_to_trash(&doc, old, DAY_MS).unwrap();
    let doc = move_to_trash(&doc, recent, 20 * DAY_MS).unwrap();

    let doc = purge_expired_trash(&doc, DAY_MS + TRASH_RETENTION_MS + 1);

    let ids: Vec<EntryId> = doc.entries.iter().map(|e| e.id).collect();
    assert_eq!(ids, [recent, kept]);
}

#[test]
fn moves_put_an_entry_exactly_where_asked() {
    let (doc, a) = add(&empty(), "a", 1, 1);
    let (doc, b) = add(&doc, "b", 2, 2);
    let (doc, c) = add(&doc, "c", 3, 3);

    let doc = move_entry(&doc, c, Placement::First, 4).unwrap();
    assert_eq!(titles(&doc), ["c", "a", "b"]);
    let doc = move_entry(&doc, c, Placement::Last, 5).unwrap();
    assert_eq!(titles(&doc), ["a", "b", "c"]);
    let doc = move_entry(&doc, c, Placement::Before(b), 6).unwrap();
    assert_eq!(titles(&doc), ["a", "c", "b"]);
    let doc = move_entry(&doc, a, Placement::After(b), 7).unwrap();
    assert_eq!(titles(&doc), ["c", "b", "a"]);
    assert_eq!(
        move_entry(&doc, a, Placement::After(EntryId([9; 16])), 8).err(),
        Some(EditError::NotFound)
    );
}

#[test]
fn pinned_entries_come_first() {
    let (doc, _) = add(&empty(), "a", 1, 1);
    let (doc, b) = add(&doc, "b", 2, 2);
    let doc = edit_entry(&doc, b, Change::Pinned(true), 3).unwrap();

    assert_eq!(titles(&doc), ["b", "a"]);
}

#[test]
fn next_hotp_steps_the_counter_of_the_same_key() {
    let counter_based = NewEntry {
        spec: hotp(),
        counter: 4,
        ..new_entry("h", 1)
    };
    let (doc, id) = add_entry(&empty(), counter_based, 1).unwrap();
    let key_id = entry(&doc, id).key.value.key_id;

    let (doc, counter) = next_hotp(&doc, id, 2).unwrap();

    assert_eq!(counter, 5);
    assert_eq!(entry(&doc, id).key.value.counter, 5);
    assert_eq!(entry(&doc, id).key.value.key_id, key_id);
    let (totp_doc, totp) = add(&empty(), "t", 2, 1);
    assert_eq!(
        next_hotp(&totp_doc, totp, 2).err(),
        Some(EditError::NotHotp)
    );
}

#[test]
fn replacing_a_key_keeps_the_old_one_as_a_copy_in_the_trash() {
    let (doc, id) = add(&empty(), "a", 1, 1);
    let doc = edit_entry(&doc, id, Change::Note("keep me".into()), 2).unwrap();

    let doc = replace_key(&doc, id, secret(2), CodeSpec::default(), 0, 3).unwrap();

    assert_eq!(entry(&doc, id).key.value.secret, secret(2));
    let copies: Vec<_> = trashed(&doc);
    assert_eq!(copies.len(), 1);
    let copy = copies[0];
    assert_eq!(copy.key.value.secret, secret(1));
    assert_eq!(copy.old_key_of.value, Some(id));
    assert_eq!(copy.title.value, "a");
    assert_eq!(copy.note.value, "keep me");
}

#[test]
fn the_old_key_copy_has_the_same_id_wherever_it_is_made() {
    let (doc, id) = add(&empty(), "a", 1, 1);

    let first = replace_key(&doc, id, secret(2), CodeSpec::default(), 0, 3).unwrap();
    let second = replace_key(&doc, id, secret(2), CodeSpec::default(), 0, 3).unwrap();

    assert_eq!(trashed(&first)[0].id, trashed(&second)[0].id);
    assert_ne!(trashed(&first)[0].id, id);
}

#[test]
fn replacing_with_a_secret_that_exists_elsewhere_is_a_duplicate() {
    let (doc, a) = add(&empty(), "a", 1, 1);
    let (doc, b) = add(&doc, "b", 2, 2);

    assert_eq!(
        replace_key(&doc, a, secret(2), CodeSpec::default(), 0, 3).err(),
        Some(EditError::Duplicate { existing: b })
    );
}

#[derive(Debug, Clone)]
enum Op {
    Add(u8),
    Replace(usize, u8),
    NextHotp(usize),
    Trash(usize),
    Restore(usize),
    Rename(usize),
    Move(usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<u8>().prop_map(Op::Add),
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Op::Replace(i, b)),
        any::<usize>().prop_map(Op::NextHotp),
        any::<usize>().prop_map(Op::Trash),
        any::<usize>().prop_map(Op::Restore),
        any::<usize>().prop_map(Op::Rename),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Move(a, b)),
    ]
}

fn pick(doc: &VaultDoc, i: usize) -> Option<EntryId> {
    (!doc.entries.is_empty()).then(|| doc.entries[i % doc.entries.len()].id)
}

proptest! {
    /// Without an explicit purge, every secret ever added is still somewhere in the vault.
    #[test]
    fn no_secret_ever_disappears(ops in proptest::collection::vec(op(), 1..60)) {
        let mut doc = empty();
        let mut seen: Vec<SecretKey> = Vec::new();
        for (step, op) in (10u64..).zip(ops) {
            let next = match op {
                Op::Add(b) => add_entry(&doc, NewEntry { spec: hotp(), ..new_entry("e", b) }, step).map(|(d, _)| d),
                Op::Replace(i, b) => match pick(&doc, i) {
                    Some(id) => replace_key(&doc, id, secret(b), hotp(), 0, step),
                    None => continue,
                },
                Op::NextHotp(i) => match pick(&doc, i) {
                    Some(id) => next_hotp(&doc, id, step).map(|(d, _)| d),
                    None => continue,
                },
                Op::Trash(i) => match pick(&doc, i) { Some(id) => move_to_trash(&doc, id, step), None => continue },
                Op::Restore(i) => match pick(&doc, i) { Some(id) => restore(&doc, id, step), None => continue },
                Op::Rename(i) => match pick(&doc, i) {
                    Some(id) => edit_entry(&doc, id, Change::Title(format!("t{step}")), step),
                    None => continue,
                },
                Op::Move(a, b) => match (pick(&doc, a), pick(&doc, b)) {
                    (Some(a), Some(b)) if a != b => move_entry(&doc, a, Placement::Before(b), step),
                    _ => continue,
                },
            };
            if let Ok(next) = next {
                doc = next;
                for e in &doc.entries {
                    if !seen.contains(&e.key.value.secret) {
                        seen.push(e.key.value.secret.clone());
                    }
                }
            }
        }
        for s in &seen {
            prop_assert!(doc.entries.iter().any(|e| &e.key.value.secret == s), "a secret disappeared");
        }
        let mut stamps: Vec<Hlc> = doc.entries.iter().map(|e| e.title.stamp).collect();
        stamps.sort();
        prop_assert!(stamps.iter().all(|s| *s <= doc.clock), "the clock is behind an entry");
    }
}

fn grouped(doc: VaultDoc) -> VaultDoc {
    VaultDoc {
        group: Some(crate::SyncGroup {
            group_id: [1; 16],
            tag_key: [2; 32],
            events: vec![],
            presence: std::collections::BTreeMap::new(),
            extra: Extra::new(),
        }),
        ..doc
    }
}

#[test]
fn in_a_sync_group_deleting_for_good_leaves_a_tombstone() {
    let (doc, a) = add(&grouped(empty()), "A", 1, DAY_MS);
    let doc = move_to_trash(&doc, a, DAY_MS).unwrap();

    let purged = purge(&doc, a, 2 * DAY_MS).unwrap();

    assert!(purged.entries.is_empty());
    let [tomb] = purged.tombstones.as_slice() else {
        panic!("expected one tombstone: {:?}", purged.tombstones);
    };
    assert_eq!(tomb.entry, a);
    assert!(tomb.stamp > doc.clock && tomb.stamp.wall_ms == 2 * DAY_MS);
    assert!(tomb.acked_by.is_empty());
    assert_eq!(purged.clock, tomb.stamp);
}

#[test]
fn in_a_sync_group_expired_trash_leaves_tombstones() {
    let (doc, a) = add(&grouped(empty()), "A", 1, DAY_MS);
    let (doc, b) = add(&doc, "B", 2, DAY_MS);
    let doc = move_to_trash(&doc, a, DAY_MS).unwrap();
    let doc = move_to_trash(&doc, b, DAY_MS).unwrap();

    let doc = purge_expired_trash(&doc, DAY_MS + TRASH_RETENTION_MS + 1);

    let mut gone: Vec<EntryId> = doc.tombstones.iter().map(|t| t.entry).collect();
    gone.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(gone, expected);
    assert!(doc.tombstones.iter().all(|t| t.stamp <= doc.clock));
    assert_ne!(doc.tombstones[0].stamp, doc.tombstones[1].stamp);
}

#[test]
fn outside_a_sync_group_deleting_leaves_no_tombstone() {
    let (doc, a) = add(&empty(), "A", 1, DAY_MS);

    assert!(purge(&doc, a, DAY_MS).unwrap().tombstones.is_empty());
}

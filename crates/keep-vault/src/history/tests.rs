use super::{
    HISTORY_MERGE_WINDOW_MS, HISTORY_VERSIONS, HistoryField, LocalDoc, decode_local, encode_local,
};
use crate::EntryId;

const A: EntryId = EntryId([1; 16]);
const B: EntryId = EntryId([2; 16]);
const MINUTE_MS: u64 = 60 * 1000;

fn values(local: &LocalDoc, entry: EntryId, field: HistoryField) -> Vec<String> {
    local
        .versions(entry, field)
        .iter()
        .map(|v| v.value.clone())
        .collect()
}

#[test]
fn records_the_value_before_an_edit() {
    let local = LocalDoc::default().record(A, HistoryField::Title, "old name", 1_000);

    assert_eq!(values(&local, A, HistoryField::Title), ["old name"]);
    assert!(local.versions(A, HistoryField::Note).is_empty());
    assert!(local.versions(B, HistoryField::Title).is_empty());
}

#[test]
fn edits_within_ten_minutes_count_as_one_version() {
    let local = LocalDoc::default()
        .record(A, HistoryField::Note, "before the burst", 0)
        .record(A, HistoryField::Note, "typing…", 3 * MINUTE_MS)
        .record(
            A,
            HistoryField::Note,
            "still typing",
            HISTORY_MERGE_WINDOW_MS - 1,
        )
        .record(A, HistoryField::Note, "next session", 30 * MINUTE_MS);

    assert_eq!(
        values(&local, A, HistoryField::Note),
        ["next session", "before the burst"]
    );
}

#[test]
fn keeps_only_the_latest_five_versions_per_field() {
    let mut local = LocalDoc::default();
    for i in 0..8u64 {
        local = local.record(A, HistoryField::Title, &format!("v{i}"), i * 60 * MINUTE_MS);
    }

    assert_eq!(
        values(&local, A, HistoryField::Title).len(),
        HISTORY_VERSIONS
    );
    assert_eq!(
        values(&local, A, HistoryField::Title),
        ["v7", "v6", "v5", "v4", "v3"]
    );
}

#[test]
fn an_unchanged_value_is_not_recorded_twice_in_a_row() {
    let local = LocalDoc::default()
        .record(A, HistoryField::Title, "same", 0)
        .record(A, HistoryField::Title, "same", 60 * MINUTE_MS);

    assert_eq!(values(&local, A, HistoryField::Title), ["same"]);
}

#[test]
fn forgetting_an_entry_drops_its_history_only() {
    let local = LocalDoc::default()
        .record(A, HistoryField::Title, "a", 0)
        .record(B, HistoryField::Title, "b", 0)
        .forget(A);

    assert!(local.versions(A, HistoryField::Title).is_empty());
    assert_eq!(values(&local, B, HistoryField::Title), ["b"]);
}

#[test]
fn survives_encoding_and_keeps_unknown_fields() {
    let mut local = LocalDoc::default()
        .record(A, HistoryField::Title, "名称 v1", 5)
        .record(A, HistoryField::Note, "备注\nline", 6);
    local.extra.insert(30, ciborium::Value::Bool(true));

    let decoded = decode_local(&encode_local(&local)).unwrap();

    assert_eq!(decoded, local);
}

#[test]
fn rejects_damaged_bytes() {
    assert!(decode_local(b"").is_err());
    assert!(decode_local(&[0xa1, 0x01, 0x00]).is_err());
}

use ciborium::Value;
use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use proptest::prelude::*;

use super::{DecodeError, decode_vault, encode_vault};
use crate::{
    DeviceId, DeviceSecrets, DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial,
    MemberEvent, MemberKind, Platform, SyncGroup, Tombstone, VaultDoc,
};

fn hlc(wall_ms: u64, logical: u32) -> Hlc {
    Hlc {
        wall_ms,
        logical,
        device: DeviceTag([7; 8]),
    }
}

fn field<T>(value: T, wall_ms: u64) -> Field<T> {
    Field {
        value,
        stamp: hlc(wall_ms, 0),
    }
}

fn sample_entry() -> Entry {
    let spec = CodeSpec::new(CodeKind::CounterBased, Digest::Sha256, 8).unwrap();
    Entry {
        id: EntryId([1; 16]),
        created_at: hlc(1_000, 0),
        title: field("Example Cloud (work)".to_owned(), 1_001),
        issuer: field("Example Cloud".to_owned(), 1_002),
        account: field("alex@example.com".to_owned(), 1_003),
        key: field(
            KeyMaterial {
                secret: SecretKey::from_bytes(b"12345678901234567890".to_vec()).unwrap(),
                spec,
                counter: 42,
                key_id: hlc(1_000, 0),
            },
            1_004,
        ),
        note: field("备注：测试账号\n第二行".to_owned(), 1_005),
        tags: field(vec!["work".to_owned(), "云".to_owned()], 1_006),
        rank: field("a0".to_owned(), 1_007),
        pinned: field(true, 1_008),
        keep_duplicate: field(false, 1_009),
        trashed_at: field(Some(hlc(2_000, 3)), 1_010),
        old_key_of: field(Some(EntryId([3; 16])), 1_011),
        extra: Extra::default(),
    }
}

fn sample_doc() -> VaultDoc {
    VaultDoc {
        vault_id: [9; 16],
        entries: vec![sample_entry()],
        tombstones: vec![Tombstone {
            entry: EntryId([2; 16]),
            stamp: hlc(3_000, 1),
            acked_by: vec![[5; 32]],
        }],
        group: None,
        device: None,
        clock: hlc(3_000, 1),
        device_tag: DeviceTag([6; 8]),
        extra: Extra::default(),
    }
}

fn to_value(doc: &VaultDoc) -> Value {
    ciborium::de::from_reader(encode_vault(doc).as_slice()).unwrap()
}

fn to_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).unwrap();
    bytes
}

fn map_mut(value: &mut Value) -> &mut Vec<(Value, Value)> {
    match value {
        Value::Map(entries) => entries,
        other => panic!("expected a map, got {other:?}"),
    }
}

fn entry_map_mut(doc: &mut Value) -> &mut Value {
    let entries = map_mut(doc)
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(2.into()))
        .map(|(_, v)| v)
        .unwrap();
    match entries {
        Value::Array(items) => &mut items[0],
        other => panic!("expected an array, got {other:?}"),
    }
}

#[test]
fn a_vault_survives_encoding_and_decoding() {
    let doc = sample_doc();

    let decoded = decode_vault(&encode_vault(&doc)).unwrap();

    assert_eq!(decoded, doc);
}

#[test]
fn an_empty_vault_survives_encoding_and_decoding() {
    let doc = VaultDoc {
        vault_id: [0; 16],
        entries: vec![],
        tombstones: vec![],
        group: None,
        device: None,
        clock: hlc(0, 0),
        device_tag: DeviceTag([0; 8]),
        extra: Extra::default(),
    };

    assert_eq!(decode_vault(&encode_vault(&doc)).unwrap(), doc);
}

#[test]
fn a_totp_entry_without_trash_survives() {
    let mut entry = sample_entry();
    entry.key.value.spec = CodeSpec::default();
    entry.key.value.counter = 0;
    entry.trashed_at = field(None, 5);
    let doc = VaultDoc {
        entries: vec![entry],
        ..sample_doc()
    };

    assert_eq!(decode_vault(&encode_vault(&doc)).unwrap(), doc);
}

#[test]
fn unknown_entry_fields_are_kept_and_written_back() {
    let mut value = to_value(&sample_doc());
    map_mut(entry_map_mut(&mut value)).push((
        Value::Integer(99.into()),
        Value::Text("from a newer version".to_owned()),
    ));

    let decoded = decode_vault(&to_bytes(&value)).unwrap();
    let rewritten = to_value(&decoded);

    assert_eq!(
        decoded.entries[0].extra.get(&99),
        Some(&Value::Text("from a newer version".to_owned()))
    );
    assert_eq!(decode_vault(&to_bytes(&rewritten)).unwrap(), decoded);
    assert!(
        map_mut(&mut rewritten.clone())
            .iter()
            .any(|(k, _)| *k == Value::Integer(2.into()))
    );
}

#[test]
fn unknown_vault_fields_are_kept_and_written_back() {
    let mut value = to_value(&sample_doc());
    map_mut(&mut value).push((Value::Integer(50.into()), Value::Bool(true)));

    let decoded = decode_vault(&to_bytes(&value)).unwrap();

    assert_eq!(decoded.extra.get(&50), Some(&Value::Bool(true)));
    let again = decode_vault(&encode_vault(&decoded)).unwrap();
    assert_eq!(again.extra.get(&50), Some(&Value::Bool(true)));
}

#[test]
fn reports_a_missing_field_by_name() {
    let mut value = to_value(&sample_doc());
    map_mut(entry_map_mut(&mut value)).retain(|(k, _)| *k != Value::Integer(3.into()));

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Missing("entry.title"))
    );
}

#[test]
fn reports_a_field_of_the_wrong_type() {
    let mut value = to_value(&sample_doc());
    for (k, v) in map_mut(entry_map_mut(&mut value)).iter_mut() {
        if *k == Value::Integer(1.into()) {
            *v = Value::Text("not bytes".to_owned());
        }
    }

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("entry.id"))
    );
}

#[test]
fn rejects_a_code_spec_out_of_range() {
    let mut doc = sample_doc();
    doc.entries[0].key.value.spec = CodeSpec::default();
    let mut value = to_value(&doc);
    // key -> [material, stamp]; material {2: spec}; spec {4: digits}
    let key = map_mut(entry_map_mut(&mut value))
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(6.into()))
        .map(|(_, v)| v)
        .unwrap();
    let Value::Array(pair) = key else { panic!() };
    let spec = map_mut(&mut pair[0])
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(2.into()))
        .map(|(_, v)| v)
        .unwrap();
    for (k, v) in map_mut(spec).iter_mut() {
        if *k == Value::Integer(4.into()) {
            *v = Value::Integer(12.into());
        }
    }

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("key.spec"))
    );
}

#[test]
fn rejects_bytes_that_are_not_cbor() {
    assert_eq!(decode_vault(&[]), Err(DecodeError::NotCbor));
    assert_eq!(decode_vault(&[0xff]), Err(DecodeError::NotCbor)); // a stray "break"
    assert_eq!(decode_vault(&[0x82, 0x01]), Err(DecodeError::NotCbor)); // array cut short
}

#[test]
fn rejects_cbor_that_is_not_a_vault() {
    // 0x6e starts a 14-byte text string, so this is valid CBOR, just not a map.
    assert_eq!(
        decode_vault(b"not cbor at all"),
        Err(DecodeError::Invalid("vault"))
    );
}

#[test]
fn debug_output_never_shows_the_secret() {
    let shown = format!("{:?}", sample_doc());

    assert!(!shown.contains("12345678901234567890"), "{shown}");
}

proptest! {
    #[test]
    fn decoding_random_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_vault(&bytes);
    }

    #[test]
    fn decoding_damaged_vaults_never_panics(index in 0usize..4096, byte in any::<u8>()) {
        let mut bytes = encode_vault(&sample_doc()).to_vec();
        let at = index % bytes.len();
        bytes[at] = byte;
        let _ = decode_vault(&bytes);
    }

    #[test]
    fn any_text_fields_survive(title in "\\PC{1,40}", note in "\\PC{0,200}", tags in proptest::collection::vec("\\PC{1,10}", 0..5)) {
        let mut doc = sample_doc();
        doc.entries[0].title.value = title;
        doc.entries[0].note.value = note;
        doc.entries[0].tags.value = tags;

        prop_assert_eq!(decode_vault(&encode_vault(&doc)).unwrap(), doc);
    }
}

/// The value stored under integer `key` in a CBOR map.
fn at(value: &mut Value, key: u64) -> &mut Value {
    map_mut(value)
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(key.into()))
        .map(|(_, v)| v)
        .unwrap()
}

/// Element `index` of a CBOR array.
fn item(value: &mut Value, index: usize) -> &mut Value {
    match value {
        Value::Array(items) => &mut items[index],
        other => panic!("expected an array, got {other:?}"),
    }
}

#[test]
fn refuses_a_different_document_format() {
    let mut value = to_value(&sample_doc());
    *at(&mut value, 0) = Value::Integer(2.into());

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::UnsupportedFormat(2))
    );
}

#[test]
fn refuses_unknown_fields_inside_key_material() {
    let mut value = to_value(&sample_doc());
    let material = item(at(entry_map_mut(&mut value), 6), 0);
    map_mut(material).push((Value::Integer(9.into()), Value::Bool(true)));

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("key"))
    );
}

#[test]
fn every_code_specification_survives() {
    let kinds = [
        CodeKind::CounterBased,
        CodeKind::TimeBased { period_s: 300 },
    ];
    let digests = [Digest::Sha1, Digest::Sha256, Digest::Sha512, Digest::Md5];
    for kind in kinds {
        for digest in digests {
            for digits in 6..=8 {
                let mut doc = sample_doc();
                doc.entries[0].key.value.spec = CodeSpec::new(kind, digest, digits).unwrap();

                assert_eq!(decode_vault(&encode_vault(&doc)).unwrap(), doc);
            }
        }
    }
}

#[test]
fn refuses_a_map_with_a_repeated_key() {
    let mut value = to_value(&sample_doc());
    map_mut(&mut value).push((Value::Integer(1.into()), Value::Bytes(vec![0; 16])));

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("vault"))
    );
}

#[test]
fn refuses_malformed_timestamps_and_fields() {
    let mut short_stamp = to_value(&sample_doc());
    *at(&mut short_stamp, 4) = Value::Array(vec![Value::Integer(1.into())]);
    let mut bare_field = to_value(&sample_doc());
    *at(entry_map_mut(&mut bare_field), 3) = Value::Text("no stamp".to_owned());
    let mut bad_secret = to_value(&sample_doc());
    *at(item(at(entry_map_mut(&mut bad_secret), 6), 0), 1) = Value::Text("x".to_owned());

    assert_eq!(
        decode_vault(&to_bytes(&short_stamp)),
        Err(DecodeError::Invalid("timestamp"))
    );
    assert_eq!(
        decode_vault(&to_bytes(&bare_field)),
        Err(DecodeError::Invalid("entry.title"))
    );
    assert_eq!(
        decode_vault(&to_bytes(&bad_secret)),
        Err(DecodeError::Invalid("key.secret"))
    );
}

fn event(kind: MemberKind, author: u8, wall_ms: u64) -> MemberEvent {
    MemberEvent {
        kind,
        author: DeviceId([author; 32]),
        stamp: hlc(wall_ms, 0),
        signature: [author.wrapping_add(1); 64],
    }
}

fn sample_group() -> SyncGroup {
    SyncGroup {
        group_id: [0x61; 16],
        tag_key: [0x62; 32],
        events: vec![
            event(
                MemberKind::Join {
                    device: DeviceId([1; 32]),
                    dx_public: [0x11; 32],
                    dx_cert: [0x12; 64],
                    platform: Platform::MacOs,
                },
                1,
                10,
            ),
            event(
                MemberKind::Join {
                    device: DeviceId([2; 32]),
                    dx_public: [0x21; 32],
                    dx_cert: [0x22; 64],
                    platform: Platform::Other(99),
                },
                1,
                20,
            ),
            event(
                MemberKind::Rename {
                    device: DeviceId([2; 32]),
                    lan_name: "书房的电脑".to_owned(),
                },
                2,
                30,
            ),
            event(
                MemberKind::Remove {
                    device: DeviceId([2; 32]),
                },
                1,
                40,
            ),
        ],
        presence: [
            (DeviceId([1; 32]), hlc(50, 0)),
            (DeviceId([2; 32]), hlc(35, 2)),
        ]
        .into_iter()
        .collect(),
        extra: Extra::default(),
    }
}

fn grouped_doc() -> VaultDoc {
    VaultDoc {
        group: Some(sample_group()),
        device: Some(DeviceSecrets {
            signing_seed: [0x71; 32].into(),
            exchange_secret: [0x72; 32].into(),
        }),
        ..sample_doc()
    }
}

#[test]
fn a_sync_group_and_device_keys_survive_encoding_and_decoding() {
    let doc = grouped_doc();

    assert_eq!(decode_vault(&encode_vault(&doc)).unwrap(), doc);
}

#[test]
fn every_platform_survives() {
    for platform in [
        Platform::MacOs,
        Platform::Windows,
        Platform::Linux,
        Platform::Ios,
        Platform::Android,
        Platform::Other(1_000),
    ] {
        let mut group = sample_group();
        group.events[0].kind = MemberKind::Join {
            device: DeviceId([1; 32]),
            dx_public: [0x11; 32],
            dx_cert: [0x12; 64],
            platform,
        };
        let doc = VaultDoc {
            group: Some(group),
            ..sample_doc()
        };
        assert_eq!(
            decode_vault(&encode_vault(&doc)).unwrap(),
            doc,
            "{platform:?}"
        );
    }
}

#[test]
fn unknown_member_kinds_and_group_fields_are_kept() {
    let mut value = to_value(&grouped_doc());
    let group = at(&mut value, 6);
    map_mut(group).push((Value::Integer(90.into()), Value::Text("future".into())));
    let events = at(group, 3);
    let first = item(events, 0);
    let kind = at(first, 1);
    *kind = Value::Map(vec![
        (Value::Integer(0.into()), Value::Integer(77.into())),
        (Value::Integer(1.into()), Value::Bytes(vec![1, 2, 3])),
    ]);

    let decoded = decode_vault(&to_bytes(&value)).unwrap();
    let group = decoded.group.as_ref().unwrap();

    assert_eq!(group.extra.get(&90), Some(&Value::Text("future".into())));
    let MemberKind::Other { kind, body } = &group.events[0].kind else {
        panic!("expected an unknown kind, got {:?}", group.events[0].kind);
    };
    assert_eq!(*kind, 77);
    assert_eq!(decode_vault(&encode_vault(&decoded)).unwrap(), decoded);
    assert!(matches!(body, Value::Map(_)));
}

#[test]
fn a_known_member_kind_with_extra_fields_is_kept_as_unknown() {
    let mut value = to_value(&grouped_doc());
    let kind = at(item(at(at(&mut value, 6), 3), 3), 1);
    map_mut(kind).push((Value::Integer(9.into()), Value::Bool(true)));

    let decoded = decode_vault(&to_bytes(&value)).unwrap();

    assert!(matches!(
        decoded.group.unwrap().events[3].kind,
        MemberKind::Other { kind: 2, .. }
    ));
}

#[test]
fn a_vault_without_a_group_reads_as_not_in_a_group() {
    let decoded = decode_vault(&encode_vault(&sample_doc())).unwrap();

    assert_eq!(decoded.group, None);
    assert_eq!(decoded.device, None);
}

#[test]
fn rejects_malformed_device_keys_and_events() {
    let mut short_seed = to_value(&grouped_doc());
    *at(at(&mut short_seed, 7), 1) = Value::Bytes(vec![1; 31]);
    let mut extra_secret_field = to_value(&grouped_doc());
    map_mut(at(&mut extra_secret_field, 7)).push((Value::Integer(9.into()), Value::Null));
    let mut short_signature = to_value(&grouped_doc());
    *at(item(at(at(&mut short_signature, 6), 3), 0), 4) = Value::Bytes(vec![0; 63]);

    assert_eq!(
        decode_vault(&to_bytes(&short_seed)),
        Err(DecodeError::Invalid("device.signing_seed"))
    );
    assert_eq!(
        decode_vault(&to_bytes(&extra_secret_field)),
        Err(DecodeError::Invalid("device"))
    );
    assert_eq!(
        decode_vault(&to_bytes(&short_signature)),
        Err(DecodeError::Invalid("event.signature"))
    );
}

#[test]
fn debug_output_never_shows_device_keys() {
    let debug = format!("{:?}", grouped_doc());

    assert!(!debug.contains("113"), "{debug}");
    assert!(
        !debug.contains("0x71") && !debug.contains("71, 71"),
        "{debug}"
    );
}

#[test]
fn a_steam_specification_survives() {
    for period_s in [30, 60] {
        let mut doc = sample_doc();
        doc.entries[0].key.value.spec = CodeSpec::steam(period_s).unwrap();

        let back = decode_vault(&encode_vault(&doc)).unwrap();

        assert_eq!(back, doc);
        assert!(back.entries[0].key.value.spec.is_steam());
    }
}

/// The spec map of the first entry: key -> [material, stamp]; material {2: spec}.
fn spec_map_mut(value: &mut Value) -> &mut Vec<(Value, Value)> {
    let key = map_mut(entry_map_mut(value))
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(6.into()))
        .map(|(_, v)| v)
        .unwrap();
    let Value::Array(pair) = key else { panic!() };
    let spec = map_mut(&mut pair[0])
        .iter_mut()
        .find(|(k, _)| *k == Value::Integer(2.into()))
        .map(|(_, v)| v)
        .unwrap();
    map_mut(spec)
}

#[test]
fn decimal_specifications_are_written_as_before() {
    let mut doc = sample_doc();
    doc.entries[0].key.value.spec = CodeSpec::default();
    let mut value = to_value(&doc);

    // No encoding field for decimal codes, so files written before Steam support keep their exact bytes.
    let keys: Vec<Value> = spec_map_mut(&mut value)
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(keys, [1, 2, 3, 4].map(|k| Value::Integer(k.into())));
}

#[test]
fn rejects_a_steam_mark_beside_other_settings() {
    let mut doc = sample_doc();
    doc.entries[0].key.value.spec = CodeSpec::steam(30).unwrap();
    let mut value = to_value(&doc);
    for (k, v) in spec_map_mut(&mut value).iter_mut() {
        if *k == Value::Integer(4.into()) {
            *v = Value::Integer(6.into());
        }
    }

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("key.spec"))
    );
}

#[test]
fn rejects_an_unknown_encoding() {
    let mut doc = sample_doc();
    doc.entries[0].key.value.spec = CodeSpec::steam(30).unwrap();
    let mut value = to_value(&doc);
    for (k, v) in spec_map_mut(&mut value).iter_mut() {
        if *k == Value::Integer(5.into()) {
            *v = Value::Integer(9.into());
        }
    }

    assert_eq!(
        decode_vault(&to_bytes(&value)),
        Err(DecodeError::Invalid("key.spec"))
    );
}

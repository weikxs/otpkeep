#![no_main]
//! Search queries never panic, and every highlight lies inside its field.
use libfuzzer_sys::fuzz_target;
use keep_find::{FieldKind, FindContext, FindIndex};
use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial};

fn field<T>(value: T) -> Field<T> {
    Field { value, stamp: Hlc { wall_ms: 1, logical: 0, device: DeviceTag([0; 8]) } }
}

fuzz_target!(|input: (&str, &str, &str)| {
    let (query, title, note) = input;
    let entry = Entry {
        id: EntryId([1; 16]),
        created_at: field(()).stamp,
        title: field(title.to_owned()),
        issuer: field("示例银行 Example".to_owned()),
        account: field("alex@example.com".to_owned()),
        key: field(KeyMaterial {
            secret: SecretKey::from_bytes(vec![1; 20]).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            key_id: field(()).stamp,
        }),
        note: field(note.to_owned()),
        tags: field(vec!["工作".to_owned()]),
        rank: field("V".to_owned()),
        pinned: field(false),
        keep_duplicate: field(false),
        trashed_at: field(None),
        old_key_of: field(None),
        extra: Extra::new(),
    };
    for hit in FindIndex::build(&[&entry]).find(query, &FindContext::default()) {
        for (kind, ranges) in hit.matched {
            let len = match kind {
                FieldKind::Title => title.chars().count(),
                FieldKind::Note => note.chars().count(),
                _ => usize::MAX,
            };
            for r in ranges {
                assert!(r.start < r.end && r.end <= len);
            }
        }
    }
});

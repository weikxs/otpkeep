use plist::{Dictionary, Uid, Value};

use super::{GpError, read_accounts_json, read_secrets};
use crate::import::Origin;

const LINK_A: &str =
    "otpauth://totp/Example%20Cloud:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud";
const LINK_B: &str = "otpauth://totp/Demo:demo-user?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

/// A binary keyed archive of `{name: url}`, built the way `NSKeyedArchiver` lays out an `NSDictionary`.
fn keyed_archive(pairs: &[(&str, &str)], mutable_strings: bool) -> Vec<u8> {
    let mut objects: Vec<Value> = vec![Value::String("$null".into())];
    let mut keys = Vec::new();
    let mut values = Vec::new();
    let string = |s: &str, objects: &mut Vec<Value>| {
        let value = if mutable_strings {
            let mut d = Dictionary::new();
            d.insert("NS.string".into(), Value::String(s.into()));
            Value::Dictionary(d)
        } else {
            Value::String(s.into())
        };
        objects.push(value);
        Value::Uid(Uid::new(objects.len() as u64 - 1))
    };
    let root_index = objects.len() as u64;
    objects.push(Value::String("placeholder".into()));
    for (name, url) in pairs {
        keys.push(string(name, &mut objects));
        values.push(string(url, &mut objects));
    }
    let mut class = Dictionary::new();
    class.insert(
        "$classname".into(),
        Value::String("NSMutableDictionary".into()),
    );
    objects.push(Value::Dictionary(class));
    let mut root = Dictionary::new();
    root.insert("NS.keys".into(), Value::Array(keys));
    root.insert("NS.objects".into(), Value::Array(values));
    root.insert(
        "$class".into(),
        Value::Uid(Uid::new(objects.len() as u64 - 1)),
    );
    objects[usize::try_from(root_index).unwrap()] = Value::Dictionary(root);
    let mut top = Dictionary::new();
    top.insert("root".into(), Value::Uid(Uid::new(root_index)));
    let mut archive = Dictionary::new();
    archive.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
    archive.insert("$version".into(), Value::Integer(100_000.into()));
    archive.insert("$top".into(), Value::Dictionary(top));
    archive.insert("$objects".into(), Value::Array(objects));
    let mut bytes = Vec::new();
    Value::Dictionary(archive)
        .to_writer_binary(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn reads_a_0_1_secrets_file_sorted_by_name() {
    let bytes = keyed_archive(&[("Zeta work", LINK_A), ("Alpha 个人", LINK_B)], false);

    let read = read_secrets(&bytes).unwrap();

    let titles: Vec<_> = read.candidates.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Alpha 个人", "Zeta work"]);
    assert_eq!(read.candidates[1].issuer, "Example Cloud");
    assert_eq!(read.candidates[1].account, "alex");
    assert!(read.problems.is_empty());
}

#[test]
fn reads_names_stored_as_mutable_strings() {
    let read = read_secrets(&keyed_archive(&[("a", LINK_A)], true)).unwrap();

    assert_eq!(read.candidates[0].title, "a");
}

#[test]
fn reports_links_it_cannot_read_by_name_and_keeps_the_rest() {
    let read = read_secrets(&keyed_archive(
        &[("good", LINK_A), ("broken", "otpauth://totp/x")],
        false,
    ))
    .unwrap();

    assert_eq!(read.candidates.len(), 1);
    assert_eq!(read.problems[0].name, "broken");
}

#[test]
fn refuses_files_that_are_not_keyed_archives_of_names_and_links() {
    let mut not_archive = Vec::new();
    Value::Array(vec![])
        .to_writer_binary(&mut not_archive)
        .unwrap();

    assert_eq!(
        read_secrets(b"not a plist").err(),
        Some(GpError::NotRecognized)
    );
    assert_eq!(
        read_secrets(&not_archive).err(),
        Some(GpError::NotRecognized)
    );
}

#[test]
fn reads_0_2_accounts_json_in_file_order() {
    let json = format!(
        r#"[{{"id":"00000000-0000-4000-8000-000000000001","name":"Zeta","url":"{LINK_A}"}},
            {{"id":"00000000-0000-4000-8000-000000000002","name":"Alpha","url":"{LINK_B}","color":"ignored"}}]"#
    );

    let read = read_accounts_json(json.as_bytes()).unwrap();

    let titles: Vec<_> = read.candidates.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Zeta", "Alpha"]);
    assert_eq!(read.candidates[1].origin, Origin::Item(1));
}

#[test]
fn reads_the_versioned_accounts_layout_too() {
    let json =
        format!(r#"{{"formatVersion":2,"accounts":[{{"id":"x","name":"A","url":"{LINK_A}"}}]}}"#);

    assert_eq!(
        read_accounts_json(json.as_bytes())
            .unwrap()
            .candidates
            .len(),
        1
    );
}

#[test]
fn refuses_json_that_is_not_an_accounts_list() {
    assert_eq!(
        read_accounts_json(b"{}").err(),
        Some(GpError::NotRecognized)
    );
    assert_eq!(
        read_accounts_json(b"[1,2]").err(),
        Some(GpError::NotRecognized)
    );
    assert_eq!(
        read_accounts_json(b"not json").err(),
        Some(GpError::NotRecognized)
    );
}

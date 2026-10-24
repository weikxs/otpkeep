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

// Files written by macOS's own NSKeyedArchiver, the same call GoldenPassport 0.1.x makes
// (`archivedData(withRootObject: [String: String])`), with demo data. Regenerate with
// `scripts/goldenpassport-fixtures.swift`.
const REAL_EXPORT: &[u8] = include_bytes!("fixtures/demo-0.1.secrets");
const REAL_EMPTY: &[u8] = include_bytes!("fixtures/empty-0.1.secrets");

#[test]
fn reads_a_secrets_file_written_by_macos_itself() {
    let read = read_secrets(REAL_EXPORT).unwrap();

    let titles: Vec<_> = read.candidates.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Demo Git", "Example Cloud", "示例银行 个人"]);
    let bank = &read.candidates[2];
    assert_eq!(
        (bank.issuer.as_str(), bank.account.as_str()),
        ("示例银行", "alex")
    );
    let git = &read.candidates[0];
    assert_eq!(
        (git.spec.digits(), git.spec.digest()),
        (8, keep_passcode::Digest::Sha256)
    );
    assert_eq!(read.problems.len(), 1);
    assert_eq!(read.problems[0].name, "Broken");
}

#[test]
fn reads_an_empty_secrets_file_written_by_macos_itself() {
    let read = read_secrets(REAL_EMPTY).unwrap();

    assert!(read.candidates.is_empty() && read.problems.is_empty());
}

mod export {
    use keep_keyuri::KeyUri;
    use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};

    use super::super::{GpExportItem, GpIncompatible, read_secrets, write_secrets};

    fn uri(byte: u8, spec: CodeSpec, issuer: &str, account: &str) -> KeyUri {
        KeyUri {
            secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
            spec,
            counter: 0,
            issuer: issuer.into(),
            account: account.into(),
        }
    }

    #[test]
    fn writes_a_file_that_reads_back_with_names_and_secrets() {
        let a = uri(1, CodeSpec::default(), "Example Cloud", "alex@example.com");
        let b = uri(2, CodeSpec::default(), "示例银行", "alex");
        let items = [
            GpExportItem {
                name: "Example Cloud",
                uri: &a,
            },
            GpExportItem {
                name: "示例银行 个人",
                uri: &b,
            },
        ];

        let export = write_secrets(&items);
        let read = read_secrets(&export.bytes).unwrap();

        assert!(export.skipped.is_empty());
        let names: Vec<_> = read.candidates.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(names, ["Example Cloud", "示例银行 个人"]);
        assert_eq!(read.candidates[1].secret, b.secret);
    }

    #[test]
    fn leaves_out_steam_guard() {
        let steam = uri(1, CodeSpec::steam(30).unwrap(), "Steam", "alex");
        let items = [GpExportItem {
            name: &steam.issuer,
            uri: &steam,
        }];

        assert_eq!(write_secrets(&items).skipped, [(0, GpIncompatible::Steam)]);
    }

    #[test]
    fn leaves_out_what_goldenpassport_would_compute_wrongly() {
        let ok = uri(1, CodeSpec::default(), "a", "a");
        let hotp = uri(
            2,
            CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap(),
            "b",
            "b",
        );
        let sha256 = uri(
            3,
            CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Sha256, 6).unwrap(),
            "c",
            "c",
        );
        let eight = uri(
            4,
            CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Sha1, 8).unwrap(),
            "d",
            "d",
        );
        let sixty = uri(
            5,
            CodeSpec::new(CodeKind::TimeBased { period_s: 60 }, Digest::Sha1, 6).unwrap(),
            "e",
            "e",
        );
        let items: Vec<GpExportItem<'_>> = [&ok, &hotp, &sha256, &eight, &sixty]
            .iter()
            .map(|u| GpExportItem {
                name: &u.issuer,
                uri: u,
            })
            .collect();

        let export = write_secrets(&items);

        assert_eq!(
            export.skipped,
            [
                (1, GpIncompatible::CounterBased),
                (2, GpIncompatible::Algorithm),
                (3, GpIncompatible::Digits),
                (4, GpIncompatible::Period)
            ]
        );
        assert_eq!(read_secrets(&export.bytes).unwrap().candidates.len(), 1);
    }

    #[test]
    fn makes_names_unique_and_never_empty() {
        let a = uri(1, CodeSpec::default(), "Demo", "x");
        let b = uri(2, CodeSpec::default(), "Demo", "y");
        let c = uri(3, CodeSpec::default(), "", "only-account");
        let items = [
            GpExportItem {
                name: "Demo",
                uri: &a,
            },
            GpExportItem {
                name: "Demo",
                uri: &b,
            },
            GpExportItem { name: " ", uri: &c },
        ];

        let read = read_secrets(&write_secrets(&items).bytes).unwrap();

        let mut names: Vec<_> = read.candidates.iter().map(|c| c.title.clone()).collect();
        names.sort();
        assert_eq!(names, ["Demo", "Demo (2)", "only-account"]);
    }

    /// GoldenPassport 0.1.x crashes on links its parser cannot take apart: every written link must have no
    /// spaces, a query, a `secret=` parameter and no `=` inside the secret, in upper-case Base32.
    #[test]
    fn written_links_satisfy_goldenpassports_parser() {
        let a = uri(7, CodeSpec::default(), "示例 银行", "a b@example.com");
        let export = write_secrets(&[GpExportItem { name: "x", uri: &a }]);
        let archive = plist::Value::from_reader(std::io::Cursor::new(&export.bytes[..])).unwrap();
        let objects = archive
            .as_dictionary()
            .unwrap()
            .get("$objects")
            .unwrap()
            .as_array()
            .unwrap();
        let links: Vec<&str> = objects
            .iter()
            .filter_map(|o| o.as_string())
            .filter(|s| s.starts_with("otpauth://"))
            .collect();

        assert_eq!(links.len(), 1);
        let link = links[0];
        assert!(!link.contains(' '), "{link}");
        let (_, query) = link.split_once('?').unwrap();
        let secret = query
            .split('&')
            .find_map(|p| p.strip_prefix("secret="))
            .unwrap();
        assert!(!secret.contains('='));
        assert!(
            secret
                .chars()
                .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)),
            "{secret}"
        );
    }
}

/// What OTPKeep writes must be importable by GoldenPassport itself: macOS's `NSKeyedUnarchiver` reads it as
/// `[String: String]` and every link passes GoldenPassport's parser (`scripts/goldenpassport-check.swift`).
#[cfg(target_os = "macos")]
#[test]
fn macos_reads_the_file_the_way_goldenpassport_imports_it() {
    use keep_keyuri::KeyUri;
    use keep_passcode::{CodeSpec, SecretKey};

    use super::{GpExportItem, write_secrets};

    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/goldenpassport-check.swift"
    );
    if std::process::Command::new("swift")
        .arg("--version")
        .output()
        .is_err()
    {
        // No Swift toolchain on this Mac: nothing to compare against.
        return;
    }
    let uris: Vec<KeyUri> = ["Example Cloud", "示例银行", "Demo Git"]
        .iter()
        .zip(1u8..)
        .map(|(issuer, byte)| KeyUri {
            secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            issuer: (*issuer).to_owned(),
            account: "alex @example.com".to_owned(),
        })
        .collect();
    let items: Vec<GpExportItem<'_>> = uris
        .iter()
        .map(|u| GpExportItem {
            name: &u.issuer,
            uri: u,
        })
        .collect();
    let dir = tempfile::Builder::new()
        .prefix("gp-export-")
        .tempdir()
        .unwrap();
    let file = dir.path().join("OTPKeep.secrets");
    std::fs::write(&file, &*write_secrets(&items).bytes).unwrap();

    let output = std::process::Command::new("swift")
        .arg(script)
        .arg(&file)
        .output()
        .unwrap();

    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(printed.trim(), "ok 3");
}

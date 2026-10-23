use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use proptest::prelude::*;

use super::{
    Assumed, BATCH_BUDGET_BYTES, ExportItem, Incompatible, MigrationFault, Unsupported,
    decode_migration_link, encode_migration,
};

const HELLO: &[u8] = b"Hello!\xDE\xAD\xBE\xEF";

/// A length-delimited protobuf field.
fn ld(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![tag, u8::try_from(bytes.len()).unwrap()];
    out.extend_from_slice(bytes);
    out
}

/// An OtpParameters message, assembled by hand from the field numbers (design/05-transfer.md §5.1).
fn params(
    secret: &[u8],
    name: &str,
    issuer: &str,
    algorithm: u8,
    digits: u8,
    kind: u8,
    counter: Option<u8>,
) -> Vec<u8> {
    let mut m = ld(0x0a, secret);
    m.extend(ld(0x12, name.as_bytes()));
    m.extend(ld(0x1a, issuer.as_bytes()));
    m.extend([0x20, algorithm, 0x28, digits, 0x30, kind]);
    if let Some(c) = counter {
        m.extend([0x38, c]);
    }
    m
}

/// A MigrationPayload with the given OtpParameters, version 1, batch `index` of `size`, batch id `id`.
fn payload(items: &[Vec<u8>], size: u8, index: u8, id: u8) -> Vec<u8> {
    let mut p: Vec<u8> = items.iter().flat_map(|m| ld(0x0a, m)).collect();
    p.extend([0x10, 0x01, 0x18, size, 0x20, index, 0x28, id]);
    p
}

fn link(payload: &[u8]) -> String {
    let data = STANDARD
        .encode(payload)
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D");
    format!("otpauth-migration://offline?data={data}")
}

#[test]
fn decodes_a_totp_account() {
    let one = params(HELLO, "alex@example.com", "Example Cloud", 1, 1, 2, None);

    let batch = decode_migration_link(&link(&payload(&[one], 1, 0, 7))).unwrap();

    assert_eq!(
        (batch.batch_id, batch.batch_size, batch.batch_index),
        (7, 1, 0)
    );
    let item = &batch.items[0];
    assert_eq!(item.secret.expose(), HELLO);
    assert_eq!(item.issuer, "Example Cloud");
    assert_eq!(item.account, "alex@example.com");
    assert_eq!(item.spec, CodeSpec::default());
    assert!(item.assumed.is_empty());
}

#[test]
fn decodes_hotp_sha256_eight_digits_with_counter() {
    let one = params(HELLO, "demo-user", "Demo Git", 2, 2, 1, Some(42));

    let batch = decode_migration_link(&link(&payload(&[one], 1, 0, 1))).unwrap();

    let item = &batch.items[0];
    assert_eq!(
        item.spec,
        CodeSpec::new(CodeKind::CounterBased, Digest::Sha256, 8).unwrap()
    );
    assert_eq!(item.counter, 42);
}

#[test]
fn unspecified_values_fall_back_to_the_defaults_and_say_so() {
    let one = params(HELLO, "a", "", 0, 0, 0, None);

    let item = &decode_migration_link(&link(&payload(&[one], 1, 0, 1)))
        .unwrap()
        .items[0];

    assert_eq!(item.spec, CodeSpec::default());
    assert_eq!(
        item.assumed,
        [Assumed::Algorithm, Assumed::Digits, Assumed::Kind]
    );
}

#[test]
fn an_unknown_value_skips_only_that_account() {
    let good = params(HELLO, "good", "", 1, 1, 2, None);
    let bad_algorithm = params(HELLO, "bad", "", 9, 1, 2, None);
    let bad_digits = params(HELLO, "bad", "", 1, 9, 2, None);
    let bad_kind = params(HELLO, "bad", "", 1, 1, 9, None);

    let batch = decode_migration_link(&link(&payload(
        &[good, bad_algorithm, bad_digits, bad_kind],
        1,
        0,
        1,
    )))
    .unwrap();

    assert_eq!(batch.items.len(), 1);
    let reasons: Vec<_> = batch
        .unreadable
        .iter()
        .map(|u| (u.index, u.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            (1, Unsupported::Algorithm(9)),
            (2, Unsupported::Digits(9)),
            (3, Unsupported::Kind(9))
        ]
    );
}

#[test]
fn an_account_without_a_secret_is_unreadable() {
    let no_secret = params(b"", "a", "", 1, 1, 2, None);

    let batch = decode_migration_link(&link(&payload(&[no_secret], 1, 0, 1))).unwrap();

    assert!(batch.items.is_empty());
    assert_eq!(batch.unreadable[0].reason, Unsupported::NoSecret);
}

#[test]
fn unknown_fields_of_every_wire_type_are_skipped() {
    let mut one = params(HELLO, "alex", "Example Cloud", 1, 1, 2, None);
    one.extend([0x78, 0x05]); // field 15, varint
    one.extend([0x82, 0x01, 0x02, b'h', b'i']); // field 16, length-delimited (two-byte tag)
    let mut p = payload(&[one], 1, 0, 1);
    p.extend([0x9a, 0x05, 0x02, b'h', b'i']); // field 83, length-delimited
    p.extend([0x35, 1, 2, 3, 4]); // field 6, 32-bit
    p.extend([0x39, 1, 2, 3, 4, 5, 6, 7, 8]); // field 7, 64-bit

    let batch = decode_migration_link(&link(&p)).unwrap();

    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].account, "alex");
}

#[test]
fn splits_names_like_labels() {
    let cases = [
        ("Example Cloud:alex", "", "Example Cloud", "alex"),
        (
            "Example Cloud: alex",
            "Example Cloud",
            "Example Cloud",
            "alex",
        ),
        ("alex", "Example Cloud", "Example Cloud", "alex"),
        ("Other:alex", "Example Cloud", "Example Cloud", "Other:alex"),
    ];
    for (name, issuer, want_issuer, want_account) in cases {
        let one = params(HELLO, name, issuer, 1, 1, 2, None);
        let item = &decode_migration_link(&link(&payload(&[one], 1, 0, 1)))
            .unwrap()
            .items[0];
        assert_eq!(
            (item.issuer.as_str(), item.account.as_str()),
            (want_issuer, want_account),
            "{name} / {issuer}"
        );
    }
}

#[test]
fn accepts_how_scanners_return_the_link() {
    let p = payload(&[params(HELLO, "a", "", 1, 1, 2, None)], 1, 0, 1);
    let raw = STANDARD.encode(&p);
    let unpadded = raw.trim_end_matches('=').to_owned();

    for text in [
        format!("otpauth-migration://offline?data={raw}"),
        format!("OTPAUTH-MIGRATION://offline?data={unpadded}"),
        format!(
            "  otpauth-migration://offline?data={}\n",
            raw.replace('+', " ")
        ),
        link(&p),
    ] {
        assert!(decode_migration_link(&text).is_ok(), "{text}");
    }
}

#[test]
fn rejects_what_is_not_a_migration_link() {
    assert_eq!(
        decode_migration_link("otpauth://totp/a?secret=AA").err(),
        Some(MigrationFault::NotAMigrationLink)
    );
    assert_eq!(
        decode_migration_link("otpauth-migration://offline?x=1").err(),
        Some(MigrationFault::DataMissing)
    );
    assert_eq!(
        decode_migration_link("otpauth-migration://offline?data=@@@").err(),
        Some(MigrationFault::BadBase64)
    );
    assert_eq!(
        decode_migration_link(&link(&[0x0a, 0x7f, 0x01])).err(),
        Some(MigrationFault::BadPayload)
    );
}

fn item(
    secret: u8,
    spec: CodeSpec,
    counter: u64,
    issuer: &str,
    account: &str,
) -> (SecretKey, CodeSpec, u64, String, String) {
    (
        SecretKey::from_bytes(vec![secret; 20]).unwrap(),
        spec,
        counter,
        issuer.to_owned(),
        account.to_owned(),
    )
}

fn as_export(items: &[(SecretKey, CodeSpec, u64, String, String)]) -> Vec<ExportItem<'_>> {
    items
        .iter()
        .map(|(secret, spec, counter, issuer, account)| ExportItem {
            secret,
            spec: *spec,
            counter: *counter,
            issuer,
            account,
        })
        .collect()
}

#[test]
fn exports_what_google_authenticator_can_take_and_reads_it_back() {
    let hotp = CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap();
    let eight = CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Sha512, 8).unwrap();
    let owned = [
        item(
            1,
            CodeSpec::default(),
            0,
            "Example Cloud",
            "alex@example.com",
        ),
        item(2, hotp, 9, "Demo Git", "demo-user"),
        item(3, eight, 0, "示例银行", "alex"),
    ];

    let export = encode_migration(&as_export(&owned), BATCH_BUDGET_BYTES).unwrap();
    let read: Vec<_> = export
        .links
        .iter()
        .flat_map(|l| decode_migration_link(l).unwrap().items)
        .collect();

    assert!(export.separate.is_empty());
    assert_eq!(read.len(), 3);
    for ((secret, spec, counter, issuer, account), got) in owned.iter().zip(&read) {
        assert_eq!(got.secret.expose(), secret.expose());
        assert_eq!((got.spec, got.counter), (*spec, *counter));
        assert_eq!((&got.issuer, &got.account), (issuer, account));
    }
    assert_eq!(
        export.algorithm_unverified,
        [2],
        "SHA-512 support in Google Authenticator is unverified (V2)"
    );
}

#[test]
fn keeps_out_what_the_format_cannot_express() {
    let sixty = CodeSpec::new(CodeKind::TimeBased { period_s: 60 }, Digest::Sha1, 6).unwrap();
    let seven = CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Sha1, 7).unwrap();
    let md5 = CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Md5, 6).unwrap();
    let owned = [
        item(1, sixty, 0, "a", "a"),
        item(2, CodeSpec::default(), 0, "b", "b"),
        item(3, seven, 0, "c", "c"),
        item(4, md5, 0, "d", "d"),
    ];

    let export = encode_migration(&as_export(&owned), BATCH_BUDGET_BYTES).unwrap();

    assert_eq!(
        export.separate,
        [
            (0, Incompatible::Period(60)),
            (2, Incompatible::Digits(7)),
            (3, Incompatible::Md5)
        ]
    );
    assert_eq!(export.links.len(), 1);
}

#[test]
fn splits_into_batches_that_fit_the_budget_and_belong_together() {
    let owned: Vec<_> = (0..40u8)
        .map(|i| {
            item(
                i,
                CodeSpec::default(),
                0,
                "Example Cloud",
                &format!("alex{i}@example.com"),
            )
        })
        .collect();

    let export = encode_migration(&as_export(&owned), BATCH_BUDGET_BYTES).unwrap();
    let batches: Vec<_> = export
        .links
        .iter()
        .map(|l| decode_migration_link(l).unwrap())
        .collect();

    assert!(batches.len() > 1);
    let id = batches[0].batch_id;
    for (index, batch) in batches.iter().enumerate() {
        assert_eq!(batch.batch_id, id);
        assert_eq!(batch.batch_size as usize, batches.len());
        assert_eq!(batch.batch_index as usize, index);
        assert!(
            batch.encoded_len <= BATCH_BUDGET_BYTES,
            "{}",
            batch.encoded_len
        );
    }
    assert_eq!(batches.iter().map(|b| b.items.len()).sum::<usize>(), 40);
}

#[test]
fn an_account_larger_than_the_budget_gets_its_own_batch() {
    let long = "x".repeat(BATCH_BUDGET_BYTES);
    let owned = [
        item(1, CodeSpec::default(), 0, "a", "a"),
        item(2, CodeSpec::default(), 0, "b", &long),
        item(3, CodeSpec::default(), 0, "c", "c"),
    ];

    let export = encode_migration(&as_export(&owned), BATCH_BUDGET_BYTES).unwrap();

    assert_eq!(export.links.len(), 3);
}

proptest! {
    #[test]
    fn decoding_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = decode_migration_link(&link(&bytes));
    }

    #[test]
    fn exports_read_back(secret in proptest::collection::vec(any::<u8>(), 1..64), issuer in "\\PC{0,20}", account in "\\PC{0,20}", counter in any::<u64>()) {
        let s = SecretKey::from_bytes(secret).unwrap();
        let hotp = CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap();
        let items = [ExportItem { secret: &s, spec: hotp, counter, issuer: &issuer, account: &account }];

        let export = encode_migration(&items, BATCH_BUDGET_BYTES).unwrap();
        let got = decode_migration_link(&export.links[0]).unwrap();

        prop_assert_eq!(got.items[0].secret.expose(), s.expose());
        prop_assert_eq!(got.items[0].counter, counter);
    }
}

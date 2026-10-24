use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use proptest::prelude::*;

use super::write_key_uri;
use crate::{KeyUri, KeyUriNote, parse_key_uri};

const HELLO: &[u8] = b"Hello!\xDE\xAD\xBE\xEF"; // Base32: JBSWY3DPEHPK3PXP

fn uri(issuer: &str, account: &str, spec: CodeSpec, counter: u64) -> KeyUri {
    KeyUri {
        secret: SecretKey::from_bytes(HELLO.to_vec()).unwrap(),
        spec,
        counter,
        issuer: issuer.to_owned(),
        account: account.to_owned(),
    }
}

fn spec(kind: CodeKind, digest: Digest, digits: u8) -> CodeSpec {
    CodeSpec::new(kind, digest, digits).unwrap()
}

#[test]
fn writes_the_canonical_link_with_defaults_left_out() {
    let link = write_key_uri(&uri(
        "Example Cloud",
        "alex@example.com",
        CodeSpec::default(),
        0,
    ));

    assert_eq!(
        *link,
        "otpauth://totp/Example%20Cloud:alex@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud"
    );
}

#[test]
fn leaves_out_the_issuer_when_there_is_none() {
    let link = write_key_uri(&uri("", "alex", CodeSpec::default(), 0));

    assert_eq!(*link, "otpauth://totp/alex?secret=JBSWY3DPEHPK3PXP");
}

#[test]
fn writes_parameters_that_differ_from_the_defaults() {
    let s = spec(CodeKind::TimeBased { period_s: 60 }, Digest::Sha256, 8);

    let link = write_key_uri(&uri("Demo", "a", s, 0));

    assert_eq!(
        *link,
        "otpauth://totp/Demo:a?secret=JBSWY3DPEHPK3PXP&issuer=Demo&algorithm=SHA256&digits=8&period=60"
    );
}

#[test]
fn writes_hotp_with_its_counter() {
    let s = spec(CodeKind::CounterBased, Digest::Sha1, 6);

    let link = write_key_uri(&uri("Demo", "a", s, 7));

    assert_eq!(
        *link,
        "otpauth://hotp/Demo:a?secret=JBSWY3DPEHPK3PXP&issuer=Demo&counter=7"
    );
}

#[test]
fn encodes_colons_inside_names() {
    let link = write_key_uri(&uri("a:b", "c:d", CodeSpec::default(), 0));

    assert_eq!(
        *link,
        "otpauth://totp/a%3Ab:c%3Ad?secret=JBSWY3DPEHPK3PXP&issuer=a%3Ab"
    );
}

#[test]
fn keeps_an_account_with_a_colon_and_no_issuer() {
    let original = uri("", "c:d", CodeSpec::default(), 0);

    let link = write_key_uri(&original);
    let parsed = parse_key_uri(&link).unwrap();

    assert_eq!(*link, "otpauth://totp/:c%3Ad?secret=JBSWY3DPEHPK3PXP");
    assert_eq!(parsed.uri.issuer, "");
    assert_eq!(parsed.uri.account, "c:d");
}

#[test]
fn encodes_non_ascii_names_as_utf8() {
    let link = write_key_uri(&uri("示例银行", "alex", CodeSpec::default(), 0));

    assert!(
        link.starts_with("otpauth://totp/%E7%A4%BA%E4%BE%8B%E9%93%B6%E8%A1%8C:alex?"),
        "{}",
        *link
    );
}

fn any_spec() -> impl Strategy<Value = CodeSpec> {
    let kind = prop_oneof![
        Just(CodeKind::CounterBased),
        (1u16..=300).prop_map(|period_s| CodeKind::TimeBased { period_s }),
    ];
    let digest = prop_oneof![
        Just(Digest::Sha1),
        Just(Digest::Sha256),
        Just(Digest::Sha512),
        Just(Digest::Md5),
    ];
    (kind, digest, 6u8..=8).prop_map(|(k, d, n)| CodeSpec::new(k, d, n).unwrap())
}

// Names as the parser leaves them: no surrounding whitespace.
fn any_name() -> impl Strategy<Value = String> {
    "\\PC{0,24}".prop_map(|s| s.trim().to_owned())
}

proptest! {
    #[test]
    fn every_written_link_parses_back_to_the_same_values(
        bytes in proptest::collection::vec(any::<u8>(), 10..64),
        spec in any_spec(),
        counter in any::<u64>(),
        issuer in any_name(),
        account in any_name(),
    ) {
        let counter = if spec.kind() == CodeKind::CounterBased { counter } else { 0 };
        let original = KeyUri {
            secret: SecretKey::from_bytes(bytes).unwrap(),
            spec,
            counter,
            issuer,
            account,
        };

        let parsed = parse_key_uri(&write_key_uri(&original)).unwrap();

        prop_assert_eq!(parsed.uri.secret.expose(), original.secret.expose());
        prop_assert_eq!(parsed.uri.spec, original.spec);
        prop_assert_eq!(parsed.uri.counter, original.counter);
        prop_assert_eq!(&parsed.uri.issuer, &original.issuer);
        prop_assert_eq!(&parsed.uri.account, &original.account);
        prop_assert!(parsed.notes.iter().all(|n| *n == KeyUriNote::ShortSecret));
    }
}

#[test]
fn writes_steam_as_its_own_link_type_so_other_apps_cannot_misread_it() {
    let uri = uri("Steam", "alex", CodeSpec::steam(30).unwrap(), 0);

    let link = write_key_uri(&uri);

    assert_eq!(
        link.as_str(),
        "otpauth://steam/Steam:alex?secret=JBSWY3DPEHPK3PXP&issuer=Steam"
    );
    assert_eq!(parse_key_uri(&link).unwrap().uri, uri);
}

#[test]
fn writes_a_steam_period_other_than_thirty() {
    let uri = uri("", "alex", CodeSpec::steam(60).unwrap(), 0);

    assert_eq!(
        write_key_uri(&uri).as_str(),
        "otpauth://steam/alex?secret=JBSWY3DPEHPK3PXP&period=60"
    );
}

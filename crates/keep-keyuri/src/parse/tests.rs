use keep_passcode::{CodeKind, CodeSpec, Digest};
use proptest::prelude::*;

use super::parse_key_uri;
use crate::{KeyUriFault, KeyUriNote, ParsedKeyUri};

// A widely used demo secret ("Hello!\xDE\xAD\xBE\xEF").
const DEMO: &str = "JBSWY3DPEHPK3PXP";

fn parse(text: &str) -> ParsedKeyUri {
    parse_key_uri(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn fault(text: &str) -> KeyUriFault {
    match parse_key_uri(text) {
        Ok(_) => panic!("{text} should not parse"),
        Err(fault) => fault,
    }
}

#[test]
fn reads_issuer_account_and_secret() {
    let parsed = parse(&format!(
        "otpauth://totp/Example%20Cloud:alex@example.com?secret={DEMO}&issuer=Example%20Cloud"
    ));

    assert_eq!(parsed.uri.issuer, "Example Cloud");
    assert_eq!(parsed.uri.account, "alex@example.com");
    assert_eq!(parsed.uri.secret.expose(), b"Hello!\xDE\xAD\xBE\xEF");
    assert_eq!(parsed.uri.spec, CodeSpec::default());
    assert_eq!(parsed.uri.counter, 0);
    assert!(parsed.notes.is_empty());
}

#[test]
fn a_secret_alone_is_enough() {
    let parsed = parse(&format!("otpauth://totp/alex?secret={DEMO}"));

    assert_eq!(parsed.uri.issuer, "");
    assert_eq!(parsed.uri.account, "alex");
}

#[test]
fn scheme_kind_and_parameter_names_ignore_case() {
    let parsed = parse(&format!("OTPAUTH://TOTP/alex?SECRET={DEMO}&Digits=8"));

    assert_eq!(parsed.uri.spec.digits(), 8);
}

#[test]
fn reads_hotp_with_its_counter() {
    let parsed = parse(&format!("otpauth://hotp/alex?secret={DEMO}&counter=5"));

    assert_eq!(parsed.uri.spec.kind(), CodeKind::CounterBased);
    assert_eq!(parsed.uri.counter, 5);
}

#[test]
fn hotp_needs_a_counter_and_totp_ignores_it() {
    assert_eq!(
        fault(&format!("otpauth://hotp/alex?secret={DEMO}")),
        KeyUriFault::CounterMissing
    );
    assert_eq!(
        parse(&format!("otpauth://totp/alex?secret={DEMO}&counter=9"))
            .uri
            .counter,
        0
    );
}

#[test]
fn reads_every_algorithm_spelling() {
    let cases = [
        ("SHA1", Digest::Sha1),
        ("sha256", Digest::Sha256),
        ("SHA-256", Digest::Sha256),
        ("SHA512", Digest::Sha512),
        ("md5", Digest::Md5),
    ];

    for (written, digest) in cases {
        let parsed = parse(&format!(
            "otpauth://totp/a?secret={DEMO}&algorithm={written}"
        ));
        assert_eq!(parsed.uri.spec.digest(), digest, "{written}");
    }
    assert_eq!(
        fault(&format!("otpauth://totp/a?secret={DEMO}&algorithm=SHA3")),
        KeyUriFault::UnsupportedAlgorithm {
            found: "SHA3".to_owned()
        }
    );
}

#[test]
fn digits_and_period_must_be_in_range() {
    for bad in ["5", "9", "abc", ""] {
        assert_eq!(
            fault(&format!("otpauth://totp/a?secret={DEMO}&digits={bad}")),
            KeyUriFault::ParamOutOfRange {
                name: "digits",
                value: bad.to_owned()
            },
            "{bad}"
        );
    }
    for bad in ["0", "301", "x"] {
        assert_eq!(
            fault(&format!("otpauth://totp/a?secret={DEMO}&period={bad}")),
            KeyUriFault::ParamOutOfRange {
                name: "period",
                value: bad.to_owned()
            },
            "{bad}"
        );
    }
    let sixty = parse(&format!("otpauth://totp/a?secret={DEMO}&period=60"));
    assert_eq!(sixty.uri.spec.kind(), CodeKind::TimeBased { period_s: 60 });
}

#[test]
fn hotp_ignores_the_period_and_checks_the_counter() {
    let parsed = parse(&format!(
        "otpauth://hotp/a?secret={DEMO}&counter=1&period=nonsense"
    ));
    assert_eq!(parsed.uri.counter, 1);
    assert_eq!(
        fault(&format!("otpauth://hotp/a?secret={DEMO}&counter=-1")),
        KeyUriFault::ParamOutOfRange {
            name: "counter",
            value: "-1".to_owned()
        }
    );
}

#[test]
fn rejects_what_is_not_an_otpauth_link() {
    assert_eq!(fault("not a link"), KeyUriFault::NotAUri);
    assert_eq!(fault("otpauth:/totp/a?secret=AA"), KeyUriFault::NotAUri);
    assert_eq!(
        fault("https://example.com/"),
        KeyUriFault::WrongScheme {
            found: "https".to_owned()
        }
    );
    assert_eq!(
        fault(&format!("otpauth://steam/a?secret={DEMO}")),
        KeyUriFault::UnsupportedKind {
            found: "steam".to_owned()
        }
    );
}

#[test]
fn the_secret_is_required() {
    assert_eq!(
        fault("otpauth://totp/a?issuer=X"),
        KeyUriFault::SecretMissing
    );
    assert_eq!(
        fault("otpauth://totp/a?secret="),
        KeyUriFault::SecretMissing
    );
    assert_eq!(
        fault("otpauth://totp/a?secret=%20"),
        KeyUriFault::SecretMissing
    );
}

#[test]
fn points_at_the_bad_character_in_the_secret() {
    assert_eq!(
        // The demo secret with a '1' (not Base32) after its first eight characters.
        fault(&format!("otpauth://totp/a?secret={}1HPK", &DEMO[..8])),
        KeyUriFault::SecretInvalid { at: 8, found: '1' }
    );
    assert_eq!(
        fault("otpauth://totp/a?secret=JB=SW"),
        KeyUriFault::SecretInvalid { at: 2, found: '=' }
    );
}

#[test]
fn tolerates_how_links_get_pasted() {
    let parsed = parse(&format!(
        "  \n otpauth://totp/Example Cloud:alex?secret={DEMO}&issuer=Example+Cloud \r\n"
    ));

    assert_eq!(parsed.uri.issuer, "Example Cloud");
    assert_eq!(parsed.uri.account, "alex");
}

#[test]
fn splits_the_label_on_any_colon_form() {
    for label in [
        "%E7%A4%BA%E4%BE%8B%E9%93%B6%E8%A1%8C:alex",
        "示例银行：alex",
        "示例银行%3Aalex",
        " 示例银行 : alex ",
    ] {
        let parsed = parse(&format!("otpauth://totp/{label}?secret={DEMO}"));
        assert_eq!(parsed.uri.issuer, "示例银行", "{label}");
        assert_eq!(parsed.uri.account, "alex", "{label}");
    }
}

#[test]
fn a_literal_colon_wins_over_an_encoded_one() {
    let parsed = parse(&format!("otpauth://totp/Demo:a%3Ab?secret={DEMO}"));

    assert_eq!(parsed.uri.issuer, "Demo");
    assert_eq!(parsed.uri.account, "a:b");
}

#[test]
fn the_issuer_parameter_wins_and_a_different_label_is_noted() {
    let parsed = parse(&format!(
        "otpauth://totp/Old%20Name:alex?secret={DEMO}&issuer=Example%20Cloud"
    ));

    assert_eq!(parsed.uri.issuer, "Example Cloud");
    assert_eq!(
        parsed.notes,
        vec![KeyUriNote::IssuerMismatch {
            in_label: "Old Name".to_owned()
        }]
    );
    let same = parse(&format!(
        "otpauth://totp/example%20cloud:alex?secret={DEMO}&issuer=Example%20Cloud"
    ));
    assert!(same.notes.is_empty());
}

#[test]
fn ignores_unknown_parameters_and_fragments() {
    let parsed = parse(&format!(
        "otpauth://totp/a?secret={DEMO}&image=https%3A%2F%2Fexample.com%2Fx.png&color=red#frag"
    ));

    assert_eq!(parsed.uri.account, "a");
    assert!(parsed.notes.is_empty());
}

#[test]
fn keeps_the_first_of_repeated_parameters() {
    let parsed = parse(&format!(
        "otpauth://totp/a?secret={DEMO}&issuer=First&issuer=Second"
    ));

    assert_eq!(parsed.uri.issuer, "First");
    assert_eq!(
        parsed.notes,
        vec![KeyUriNote::DuplicateParam {
            name: "issuer".to_owned()
        }]
    );
}

#[test]
fn notes_a_short_secret() {
    let parsed = parse("otpauth://totp/a?secret=MFRGG");

    assert_eq!(parsed.notes, vec![KeyUriNote::ShortSecret]);
}

#[test]
fn accepts_a_missing_label() {
    for link in [
        format!("otpauth://totp/?secret={DEMO}&issuer=Demo"),
        format!("otpauth://totp?secret={DEMO}&issuer=Demo"),
    ] {
        let parsed = parse(&link);
        assert_eq!(parsed.uri.issuer, "Demo", "{link}");
        assert_eq!(parsed.uri.account, "", "{link}");
    }
}

#[test]
fn debug_output_never_shows_the_secret() {
    let parsed = parse(&format!("otpauth://totp/a?secret={DEMO}"));

    let shown = format!("{parsed:?}");

    assert!(!shown.contains(DEMO), "{shown}");
    assert!(!shown.contains("Hello"), "{shown}");
}

proptest! {
    #[test]
    fn never_panics_on_any_text(text in "\\PC*") {
        let _ = parse_key_uri(&text);
    }

    #[test]
    fn never_panics_on_link_shaped_text(rest in "\\PC*") {
        let _ = parse_key_uri(&format!("otpauth://totp/{rest}"));
        let _ = parse_key_uri(&format!("otpauth://hotp/a?{rest}"));
    }
}

#[test]
fn a_malformed_scheme_or_missing_kind_is_not_a_link() {
    assert_eq!(fault("my scheme://x"), KeyUriFault::NotAUri);
    assert_eq!(fault("://x"), KeyUriFault::NotAUri);
    assert_eq!(
        fault(&format!("otpauth:///a?secret={DEMO}")),
        KeyUriFault::NotAUri
    );
}

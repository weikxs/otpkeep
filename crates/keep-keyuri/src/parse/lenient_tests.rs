//! How links arrive in practice: copied from chats, mails and web pages, typed with a Chinese input method, or
//! written by other apps (behaviour compared with the Aegis and FreeOTP apps).

use keep_passcode::{CodeKind, Digest};

use super::parse_key_uri;
use crate::{KeyUriFault, KeyUriNote, ParsedKeyUri};

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
fn ignores_invisible_characters_from_copying() {
    let parsed = parse(
        "\u{feff}otpauth://totp/Demo:alex?secret=JBSW\u{200b}Y3DPEHPK3PXP&issuer=Demo\u{200d}",
    );

    assert_eq!(parsed.uri.secret.expose(), b"Hello!\xDE\xAD\xBE\xEF");
    assert_eq!(parsed.uri.issuer, "Demo");
}

#[test]
fn finds_the_link_inside_surrounding_text() {
    for text in [
        format!("请扫描或复制：otpauth://totp/Demo:alex?secret={DEMO}"),
        format!("1. otpauth://totp/Demo:alex?secret={DEMO}"),
        format!("- otpauth://totp/Demo:alex?secret={DEMO}"),
        format!("Your key: OTPAUTH://totp/Demo:alex?secret={DEMO}\nThanks"),
    ] {
        let parsed = parse(&text);
        assert_eq!(parsed.uri.account, "alex", "{text}");
    }
}

#[test]
fn removes_quotes_and_brackets_around_the_link() {
    for text in [
        format!("\"otpauth://totp/Demo:alex?secret={DEMO}\""),
        format!("“otpauth://totp/Demo:alex?secret={DEMO}”"),
        format!("<otpauth://totp/Demo:alex?secret={DEMO}>"),
        format!("（otpauth://totp/Demo:alex?secret={DEMO}）"),
        format!("「otpauth://totp/Demo:alex?secret={DEMO}」"),
        format!("otpauth://totp/Demo:alex?secret={DEMO}。"),
    ] {
        let parsed = parse(&text);
        assert_eq!(
            parsed.uri.secret.expose(),
            b"Hello!\xDE\xAD\xBE\xEF",
            "{text}"
        );
    }
}

#[test]
fn keeps_a_closing_bracket_that_belongs_to_the_link() {
    let parsed = parse(&format!(
        "otpauth://totp/alex?secret={DEMO}&issuer=Demo%20(EU)"
    ));

    assert_eq!(parsed.uri.issuer, "Demo (EU)");
}

#[test]
fn reads_full_width_letters_and_digits_in_values() {
    let parsed = parse(
        "otpauth://totp/示例：alex?secret=ＪＢＳＷＹ３ＤＰＥＨＰＫ３ＰＸＰ&digits=８&period=６０",
    );

    assert_eq!(parsed.uri.secret.expose(), b"Hello!\xDE\xAD\xBE\xEF");
    assert_eq!(parsed.uri.spec.digits(), 8);
    assert_eq!(parsed.uri.spec.kind(), CodeKind::TimeBased { period_s: 60 });
}

#[test]
fn leaves_names_as_they_are_written() {
    let parsed = parse(&format!(
        "otpauth://totp/示例（中国）:ａｌｅｘ?secret={DEMO}"
    ));

    assert_eq!(parsed.uri.issuer, "示例（中国）");
    assert_eq!(parsed.uri.account, "ａｌｅｘ");
}

#[test]
fn reads_algorithm_names_as_programs_write_them() {
    for (written, digest) in [
        ("HmacSHA256", Digest::Sha256),
        ("HMAC-SHA512", Digest::Sha512),
        ("hmac_sha1", Digest::Sha1),
        ("sha_256", Digest::Sha256),
        ("ＳＨＡ２５６", Digest::Sha256),
    ] {
        let parsed = parse(&format!(
            "otpauth://totp/alex?secret={DEMO}&algorithm={written}"
        ));
        assert_eq!(parsed.uri.spec.digest(), digest, "{written}");
    }
}

#[test]
fn an_hotp_link_without_a_counter_starts_at_zero() {
    let parsed = parse(&format!("otpauth://hotp/alex?secret={DEMO}"));

    assert_eq!(parsed.uri.spec.kind(), CodeKind::CounterBased);
    assert_eq!(parsed.uri.counter, 0);
    assert!(parsed.notes.contains(&KeyUriNote::CounterAssumedZero));
}

#[test]
fn an_unreadable_counter_is_still_refused() {
    assert!(matches!(
        fault(&format!("otpauth://hotp/alex?secret={DEMO}&counter=-1")),
        KeyUriFault::ParamOutOfRange {
            name: "counter",
            ..
        }
    ));
}

#[test]
fn names_a_google_authenticator_export() {
    assert_eq!(
        fault("otpauth-migration://offline?data=CjEKCkhlbGxvId6tvu8"),
        KeyUriFault::GoogleExport
    );
}

#[test]
fn names_a_microsoft_push_code() {
    assert_eq!(
        fault(
            "phonefactor://activate_account?code=123456789&url=https%3A%2F%2Fexample.invalid%2Fpad"
        ),
        KeyUriFault::MicrosoftPush
    );
}

#[test]
fn names_an_oracle_enrolment_code() {
    assert_eq!(
        fault(
            "oraclemobileauthenticator://totp/demo%40example.com?issuer=demo&OTP=eyJ.x.y&LoginURL=https%3A%2F%2Fexample.invalid"
        ),
        KeyUriFault::OracleEnrolment
    );
}

#[test]
fn names_a_bare_secret() {
    for text in [DEMO, "jbsw y3dp ehpk 3pxp", "JBSW-Y3DP-EHPK-3PXP"] {
        assert_eq!(fault(text), KeyUriFault::BareSecret, "{text}");
    }
}

#[test]
fn ordinary_words_are_not_a_secret() {
    for text in ["hello", "not a link at all", "密钥", "ABC"] {
        assert_eq!(fault(text), KeyUriFault::NotAUri, "{text}");
    }
}

#[test]
fn another_kind_of_link_is_still_a_wrong_scheme() {
    assert!(matches!(
        fault("https://example.com/setup?secret=JBSWY3DPEHPK3PXP"),
        KeyUriFault::WrongScheme { .. }
    ));
}

#[test]
fn reads_a_typed_secret_the_same_way() {
    for typed in [
        "JBSWY3DPEHPK3PXP",
        "jbsw y3dp ehpk 3pxp",
        "ＪＢＳＷ-Ｙ３ＤＰ-ＥＨＰＫ-３ＰＸＰ",
        "JBSWY3DPEHPK3PXP====",
    ] {
        assert_eq!(
            crate::parse_secret(typed).unwrap().expose(),
            b"Hello!\xDE\xAD\xBE\xEF",
            "{typed}"
        );
    }
    assert_eq!(crate::parse_secret("  "), Err(KeyUriFault::SecretMissing));
    assert_eq!(
        crate::parse_secret("JBSW0"),
        Err(KeyUriFault::SecretInvalid { at: 4, found: '0' })
    );
}

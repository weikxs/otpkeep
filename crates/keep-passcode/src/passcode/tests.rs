use proptest::prelude::*;

use super::{Moment, NEXT_CODE_WINDOW_S, PasscodeError, derive_passcode};
use crate::{CodeKind, CodeSpec, Digest, SecretKey};

// Test secrets from RFC 4226 appendix D and RFC 6238 appendix B.
const RFC_SECRET_20: &[u8] = b"12345678901234567890";
const RFC_SECRET_32: &[u8] = b"12345678901234567890123456789012";
const RFC_SECRET_64: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

fn secret(bytes: &[u8]) -> SecretKey {
    SecretKey::from_bytes(bytes.to_vec()).unwrap_or_else(|e| panic!("{e}"))
}

fn spec(kind: CodeKind, digest: Digest, digits: u8) -> CodeSpec {
    CodeSpec::new(kind, digest, digits).unwrap_or_else(|e| panic!("{e}"))
}

fn totp(period_s: u16) -> CodeKind {
    CodeKind::TimeBased { period_s }
}

fn derive(key: &SecretKey, spec: CodeSpec, moment: Moment) -> super::Passcode {
    derive_passcode(key, spec, moment).unwrap_or_else(|e| panic!("{e}"))
}

fn code(key: &SecretKey, spec: CodeSpec, moment: Moment) -> String {
    derive(key, spec, moment).code().to_owned()
}

#[test]
fn matches_rfc_4226_hotp_vectors() {
    let expected = [
        "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871",
        "520489",
    ];
    let key = secret(RFC_SECRET_20);
    let hotp = spec(CodeKind::CounterBased, Digest::Sha1, 6);

    for (counter, want) in (0u64..).zip(expected) {
        assert_eq!(
            code(&key, hotp, Moment::Counter(counter)),
            want,
            "counter {counter}"
        );
    }
}

#[test]
fn matches_rfc_6238_totp_vectors() {
    let rows: [(i64, &str, &str, &str); 6] = [
        (59, "94287082", "46119246", "90693936"),
        (1_111_111_109, "07081804", "68084774", "25091201"),
        (1_111_111_111, "14050471", "67062674", "99943326"),
        (1_234_567_890, "89005924", "91819424", "93441116"),
        (2_000_000_000, "69279037", "90698825", "38618901"),
        (20_000_000_000, "65353130", "77737706", "47863826"),
    ];
    let keys = [
        (secret(RFC_SECRET_20), Digest::Sha1),
        (secret(RFC_SECRET_32), Digest::Sha256),
        (secret(RFC_SECRET_64), Digest::Sha512),
    ];

    for (time, sha1, sha256, sha512) in rows {
        for ((key, digest), want) in keys.iter().zip([sha1, sha256, sha512]) {
            let s = spec(totp(30), *digest, 8);
            assert_eq!(
                code(key, s, Moment::Time(time)),
                want,
                "{digest:?} at {time}"
            );
        }
    }
}

// Reference values computed with an independent implementation (Python's hmac module).
#[test]
fn honors_digits_and_periods_other_than_the_defaults() {
    let key = secret(RFC_SECRET_20);
    let cases = [
        (spec(totp(30), Digest::Sha1, 7), 59, "4287082"),
        (spec(totp(60), Digest::Sha1, 6), 1_111_111_109, "360094"),
        (spec(totp(1), Digest::Sha1, 6), 1_234_567_890, "965462"),
        (spec(totp(300), Digest::Sha1, 6), 2_000_000_000, "473453"),
    ];

    for (s, time, want) in cases {
        assert_eq!(code(&key, s, Moment::Time(time)), want, "{s:?} at {time}");
    }
}

#[test]
fn keeps_leading_zeros_and_handles_the_largest_counter() {
    let key = secret(RFC_SECRET_20);
    let hotp = spec(CodeKind::CounterBased, Digest::Sha1, 6);

    assert_eq!(code(&key, hotp, Moment::Counter(u64::MAX)), "094451");
}

#[test]
fn reports_how_long_a_totp_code_stays_valid() {
    let passcode = derive(&secret(RFC_SECRET_20), CodeSpec::default(), Moment::Time(0));

    assert_eq!(passcode.valid_for_s(), Some(30));
    assert_eq!(passcode.next(), None);
}

#[test]
fn gives_the_next_code_in_the_last_five_seconds() {
    let key = secret(RFC_SECRET_20);
    let s = CodeSpec::default();
    let window_start = 30 - i64::from(NEXT_CODE_WINDOW_S);

    let before = derive(&key, s, Moment::Time(window_start - 1));
    let inside = derive(&key, s, Moment::Time(window_start));

    assert_eq!(before.next(), None);
    assert_eq!(inside.valid_for_s(), Some(NEXT_CODE_WINDOW_S));
    assert_eq!(inside.code(), "755224");
    assert_eq!(inside.next(), Some("287082"));
}

#[test]
fn hotp_codes_have_no_expiry_and_no_next_code() {
    let hotp = spec(CodeKind::CounterBased, Digest::Sha1, 6);

    let passcode = derive(&secret(RFC_SECRET_20), hotp, Moment::Counter(3));

    assert_eq!(passcode.valid_for_s(), None);
    assert_eq!(passcode.next(), None);
}

#[test]
fn times_before_1970_saturate_to_the_epoch() {
    let key = secret(RFC_SECRET_20);
    let s = CodeSpec::default();

    for time in [-1, -59, i64::MIN] {
        assert_eq!(
            code(&key, s, Moment::Time(time)),
            code(&key, s, Moment::Time(0)),
            "{time}"
        );
    }
}

#[test]
fn the_largest_time_does_not_panic() {
    let key = secret(RFC_SECRET_20);

    for period_s in [1, 30, 300] {
        let s = spec(totp(period_s), Digest::Sha1, 6);
        assert_eq!(code(&key, s, Moment::Time(i64::MAX)).len(), 6);
    }
}

#[test]
fn md5_is_refused_because_its_digest_is_too_short_to_truncate() {
    let s = spec(CodeKind::CounterBased, Digest::Md5, 6);

    assert_eq!(
        derive_passcode(&secret(RFC_SECRET_20), s, Moment::Counter(0)).err(),
        Some(PasscodeError::UnsupportedDigest(Digest::Md5))
    );
}

#[test]
fn a_moment_must_match_the_kind_of_code() {
    let key = secret(RFC_SECRET_20);
    let hotp = spec(CodeKind::CounterBased, Digest::Sha1, 6);

    assert_eq!(
        derive_passcode(&key, CodeSpec::default(), Moment::Counter(1)).err(),
        Some(PasscodeError::MomentMismatch)
    );
    assert_eq!(
        derive_passcode(&key, hotp, Moment::Time(1)).err(),
        Some(PasscodeError::MomentMismatch)
    );
}

#[test]
fn debug_output_never_shows_the_code() {
    let passcode = derive(
        &secret(RFC_SECRET_20),
        CodeSpec::default(),
        Moment::Time(27),
    );

    let shown = format!("{passcode:?}");

    assert!(!shown.contains("755224"), "{shown}");
    assert!(!shown.contains("287082"), "{shown}");
}

proptest! {
    #[test]
    fn every_code_has_exactly_the_requested_digits(
        bytes in proptest::collection::vec(any::<u8>(), 1..80),
        counter in any::<u64>(),
        digits in 6u8..=8,
        digest in prop_oneof![Just(Digest::Sha1), Just(Digest::Sha256), Just(Digest::Sha512)],
    ) {
        let key = SecretKey::from_bytes(bytes).unwrap_or_else(|e| panic!("{e}"));
        let s = spec(CodeKind::CounterBased, digest, digits);

        let text = code(&key, s, Moment::Counter(counter));

        prop_assert_eq!(text.len(), usize::from(digits));
        prop_assert!(text.bytes().all(|b| b.is_ascii_digit()));
    }
}

/// steamguard-cli's published test: this secret at 1616374841 gives `2F9J5`.
const STEAM_SECRET: &[u8] = &[
    0xce, 0xf2, 0x1a, 0xca, 0x9d, 0xc9, 0x3e, 0xfb, 0x6f, 0x5f, 0xf4, 0x06, 0x1e, 0xab, 0x2a, 0x28,
    0x19, 0x3f, 0xe3, 0x8b,
];

#[test]
fn matches_the_steam_guard_reference_code() {
    let key = secret(STEAM_SECRET);
    let spec = CodeSpec::steam(30).unwrap();

    assert_eq!(code(&key, spec, Moment::Time(1_616_374_841)), "2F9J5");
}

#[test]
fn steam_codes_use_only_the_steam_alphabet_and_offer_the_next_one() {
    let key = secret(STEAM_SECRET);
    let spec = CodeSpec::steam(30).unwrap();

    let passcode = derive(&key, spec, Moment::Time(1_616_374_858));

    assert_eq!(passcode.valid_for_s(), Some(2));
    for text in [passcode.code(), passcode.next().unwrap()] {
        assert_eq!(text.len(), 5);
        assert!(
            text.chars()
                .all(|c| "23456789BCDFGHJKMNPQRTVWXY".contains(c)),
            "{text}"
        );
    }
}

//! Steam Guard links, in the three ways apps write them: `otpauth://steam/` (Aegis, 2FAuth, Ente),
//! `encoder=steam` (KeePassXC, steamguard-cli, Strongbox) and a `Steam` issuer (FreeOTP).

use keep_passcode::{CodeKind, Encoding};

use super::parse_key_uri;
use crate::KeyUriFault;

const DEMO: &str = "JBSWY3DPEHPK3PXP";

fn encoding(text: &str) -> Encoding {
    parse_key_uri(text)
        .unwrap_or_else(|e| panic!("{text}: {e}"))
        .uri
        .spec
        .encoding()
}

#[test]
fn reads_the_steam_link_type() {
    let parsed = parse_key_uri(&format!("otpauth://steam/Steam:alex?secret={DEMO}")).unwrap();

    assert!(parsed.uri.spec.is_steam());
    assert_eq!(parsed.uri.spec.kind(), CodeKind::TimeBased { period_s: 30 });
    assert_eq!(parsed.uri.issuer, "Steam");
    assert_eq!(parsed.uri.account, "alex");
}

#[test]
fn reads_the_steam_encoder_parameter() {
    let text = format!(
        "otpauth://totp/Steam:alex?secret={DEMO}&period=30&digits=5&issuer=Steam&encoder=steam"
    );

    assert_eq!(encoding(&text), Encoding::Steam);
}

#[test]
fn the_encoder_counts_whatever_the_issuer() {
    let text = format!("otpauth://totp/alex%20steam:alex?secret={DEMO}&encoder=STEAM");

    assert_eq!(encoding(&text), Encoding::Steam);
}

#[test]
fn a_steam_issuer_means_steam_guard_as_freeotp_reads_it() {
    for text in [
        format!("otpauth://totp/Steam:alex?secret={DEMO}"),
        format!("otpauth://totp/alex?secret={DEMO}&issuer=steam"),
        format!("otpauth://totp/Steam:alex?secret={DEMO}&digits=5"),
    ] {
        assert_eq!(encoding(&text), Encoding::Steam, "{text}");
    }
}

#[test]
fn a_steam_issuer_with_decimal_digits_stays_decimal() {
    let text = format!("otpauth://totp/Steam:alex?secret={DEMO}&digits=6");

    assert_eq!(encoding(&text), Encoding::Decimal);
}

#[test]
fn keeps_a_steam_period() {
    let parsed = parse_key_uri(&format!("otpauth://steam/alex?secret={DEMO}&period=60")).unwrap();

    assert_eq!(parsed.uri.spec.kind(), CodeKind::TimeBased { period_s: 60 });
}

#[test]
fn refuses_steam_with_another_algorithm_or_length() {
    assert!(matches!(
        parse_key_uri(&format!(
            "otpauth://steam/alex?secret={DEMO}&algorithm=SHA256"
        )),
        Err(KeyUriFault::UnsupportedAlgorithm { .. })
    ));
    assert!(matches!(
        parse_key_uri(&format!("otpauth://steam/alex?secret={DEMO}&digits=6")),
        Err(KeyUriFault::ParamOutOfRange { name: "digits", .. })
    ));
}

#[test]
fn counter_codes_are_never_steam() {
    let text = format!("otpauth://hotp/Steam:alex?secret={DEMO}&counter=1&encoder=steam");

    assert_eq!(encoding(&text), Encoding::Decimal);
}

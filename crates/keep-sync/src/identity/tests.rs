use keep_vault::{DeviceId, DeviceTag};

use super::{
    DeviceIdentity, IdentityError, device_tag, full_fingerprint, short_fingerprint, verify_dx_cert,
};

const GROUP: [u8; 16] = [0x42; 16];

fn fixed() -> DeviceIdentity {
    DeviceIdentity::from_seeds(&[1; 32], &[2; 32], &GROUP)
}

#[test]
fn a_new_identity_is_random_and_its_certificate_verifies() {
    let first = DeviceIdentity::generate(&GROUP).unwrap();
    let second = DeviceIdentity::generate(&GROUP).unwrap();

    assert_ne!(first.device_id(), second.device_id());
    assert_ne!(first.dx_public(), second.dx_public());
    assert!(verify_dx_cert(
        first.device_id(),
        &GROUP,
        &first.dx_public(),
        &first.dx_cert()
    ));
}

#[test]
fn an_identity_restored_from_its_stored_keys_is_the_same() {
    let original = DeviceIdentity::generate(&GROUP).unwrap();

    let restored = DeviceIdentity::from_secrets(&original.secrets(), &GROUP);

    assert_eq!(restored.device_id(), original.device_id());
    assert_eq!(restored.dx_public(), original.dx_public());
    assert_eq!(restored.dx_cert(), original.dx_cert());
}

#[test]
fn a_certificate_binds_the_exchange_key_to_the_device_and_the_group() {
    let device = fixed();
    let other = DeviceIdentity::from_seeds(&[3; 32], &[4; 32], &GROUP);
    let cert = device.dx_cert();

    assert!(verify_dx_cert(
        device.device_id(),
        &GROUP,
        &device.dx_public(),
        &cert
    ));
    assert!(!verify_dx_cert(
        other.device_id(),
        &GROUP,
        &device.dx_public(),
        &cert
    ));
    assert!(!verify_dx_cert(
        device.device_id(),
        &[0x43; 16],
        &device.dx_public(),
        &cert
    ));
    assert!(!verify_dx_cert(
        device.device_id(),
        &GROUP,
        &other.dx_public(),
        &cert
    ));
    let mut tampered = cert;
    tampered[0] ^= 1;
    assert!(!verify_dx_cert(
        device.device_id(),
        &GROUP,
        &device.dx_public(),
        &tampered
    ));
    assert!(!verify_dx_cert(
        DeviceId([0; 32]),
        &GROUP,
        &device.dx_public(),
        &cert
    ));
}

#[test]
fn signatures_are_checked_against_the_device_and_the_message() {
    let device = fixed();
    let signature = device.sign(b"message");

    assert!(super::verify(device.device_id(), b"message", &signature));
    assert!(!super::verify(device.device_id(), b"massage", &signature));
    assert!(!super::verify(
        DeviceIdentity::from_seeds(&[3; 32], &[4; 32], &GROUP).device_id(),
        b"message",
        &signature
    ));
}

#[test]
fn tags_and_fingerprints_come_from_a_hash_of_the_public_key() {
    let id = DeviceId([7; 32]);
    let hash = blake3::hash(&id.0);
    let bytes = hash.as_bytes();

    assert_eq!(device_tag(id), DeviceTag(bytes[..8].try_into().unwrap()));
    assert_eq!(
        short_fingerprint(id),
        format!("{:02X}{:02X}", bytes[0], bytes[1])
    );
    let full = full_fingerprint(id);
    assert_eq!(full.len(), 24);
    assert_eq!(full.split('-').count(), 5);
    assert!(full.starts_with(&short_fingerprint(id)));
    assert!(
        full.chars()
            .all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
    );
}

#[test]
fn fingerprints_of_known_keys_do_not_change() {
    // Pinned so a refactoring cannot silently change what users compare between devices.
    let id = fixed().device_id();

    assert_eq!(full_fingerprint(id), PINNED_FULL);
    assert_eq!(short_fingerprint(id), &PINNED_FULL[..4]);
}

const PINNED_FULL: &str = "8356-1ADB-398F-D87F-8E7E";

#[test]
fn debug_output_shows_no_keys() {
    let debug = format!("{:?}", fixed());

    assert!(
        debug.contains(&short_fingerprint(fixed().device_id())),
        "{debug}"
    );
    assert!(!debug.contains("[1, 1"), "{debug}");
}

#[test]
fn the_error_type_says_what_failed() {
    assert_eq!(
        IdentityError::RandomUnavailable.to_string(),
        "the system random number generator is unavailable"
    );
}

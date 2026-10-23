use super::{SHORT_SECRET_BYTES, SecretError, SecretKey};

fn key(bytes: &[u8]) -> SecretKey {
    SecretKey::from_bytes(bytes.to_vec()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn an_empty_secret_is_rejected() {
    assert_eq!(
        SecretKey::from_bytes(Vec::new()).err(),
        Some(SecretError::Empty)
    );
}

#[test]
fn a_secret_keeps_its_bytes() {
    assert_eq!(
        key(b"12345678901234567890").expose(),
        b"12345678901234567890"
    );
}

#[test]
fn secrets_shorter_than_ten_bytes_are_short() {
    assert!(key(&[7; SHORT_SECRET_BYTES - 1]).is_short());
    assert!(!key(&[7; SHORT_SECRET_BYTES]).is_short());
}

#[test]
fn debug_output_never_shows_the_secret() {
    let shown = format!("{:?}", key(b"ABCDEFGHIJ"));

    assert!(!shown.contains("ABCDEFGHIJ"), "{shown}");
    assert!(!shown.contains("65"), "{shown}"); // no byte values either
    assert!(shown.contains("redacted"), "{shown}");
}

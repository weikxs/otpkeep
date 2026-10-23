use ciborium::Value;
use proptest::prelude::*;

use super::{
    DataKey, FileKind, KdfParams, KeySource, MAGIC, OpenError, SEALED_FORMAT, SealedHeader, open,
    read_header, seal,
};

fn key(byte: u8) -> DataKey {
    DataKey::from_bytes([byte; 32])
}

fn header() -> SealedHeader {
    SealedHeader::new(FileKind::Vault, KeySource::Keystore, [3; 16])
}

fn sealed(plaintext: &[u8]) -> Vec<u8> {
    seal(&header(), &key(1), plaintext).unwrap()
}

#[test]
fn opens_what_it_seals() {
    let bytes = sealed(b"vault contents");

    let (read, plaintext) = open(&bytes, &key(1)).unwrap();

    assert_eq!(&*plaintext, b"vault contents");
    assert_eq!(read, header());
}

#[test]
fn starts_with_the_magic_and_never_contains_the_plaintext() {
    let bytes = sealed(b"PLAINTEXT-MARKER");

    assert_eq!(&bytes[..4], MAGIC);
    assert!(!bytes.windows(16).any(|w| w == b"PLAINTEXT-MARKER"));
}

#[test]
fn every_seal_uses_a_new_nonce() {
    assert_ne!(sealed(b"same"), sealed(b"same"));
}

#[test]
fn the_wrong_key_does_not_open_it() {
    let bytes = sealed(b"secret");

    assert_eq!(open(&bytes, &key(2)).err(), Some(OpenError::AuthFailed));
}

#[test]
fn the_header_is_readable_without_the_key() {
    let master = SealedHeader {
        kdf: Some(KdfParams {
            salt: [8; 16],
            m_cost_kib: 65_536,
            t_cost: 3,
            p_cost: 1,
        }),
        ..SealedHeader::new(FileKind::Local, KeySource::MasterPassword, [0; 16])
    };
    let bytes = seal(&master, &key(1), b"x").unwrap();

    assert_eq!(read_header(&bytes).unwrap(), master);
}

#[test]
fn unknown_header_fields_survive_resealing() {
    let mut with_extra = header();
    with_extra
        .extra
        .insert(40, Value::Text("newer version".to_owned()));
    let bytes = seal(&with_extra, &key(1), b"x").unwrap();

    let read = read_header(&bytes).unwrap();
    let resealed = seal(&read, &key(1), b"y").unwrap();

    assert_eq!(
        read_header(&resealed).unwrap().extra.get(&40),
        Some(&Value::Text("newer version".to_owned()))
    );
}

#[test]
fn reports_a_newer_format_without_trying_to_open_it() {
    let newer = SealedHeader {
        format: SEALED_FORMAT + 1,
        ..header()
    };
    let bytes = seal(&newer, &key(1), b"x").unwrap();

    assert_eq!(read_header(&bytes).unwrap().format, SEALED_FORMAT + 1);
    assert_eq!(
        open(&bytes, &key(1)).err(),
        Some(OpenError::UnsupportedFormat(SEALED_FORMAT + 1))
    );
}

#[test]
fn rejects_files_that_are_not_sealed_vaults() {
    assert_eq!(read_header(b"").err(), Some(OpenError::TooShort));
    assert_eq!(read_header(b"OKV").err(), Some(OpenError::TooShort));
    assert_eq!(
        read_header(b"PK\x03\x04 a zip file").err(),
        Some(OpenError::BadMagic)
    );
    let mut cut = sealed(b"x");
    cut.truncate(10);
    assert!(matches!(
        read_header(&cut).err(),
        Some(OpenError::TooShort | OpenError::HeaderInvalid)
    ));
}

#[test]
fn a_tampered_header_is_detected() {
    let mut bytes = sealed(b"x");
    // The key id sits inside the header; flipping it must fail authentication.
    let at = bytes.windows(16).position(|w| w == [3; 16]).unwrap();
    bytes[at] ^= 0x01;

    assert_eq!(open(&bytes, &key(1)).err(), Some(OpenError::AuthFailed));
}

#[test]
fn debug_output_never_shows_the_data_key() {
    let shown = format!("{:?}", DataKey::from_bytes([0xAB; 32]));

    assert!(!shown.to_lowercase().contains("ab, "), "{shown}");
    assert!(!shown.contains("171"), "{shown}");
    assert!(shown.contains("redacted"), "{shown}");
}

#[test]
fn generated_data_keys_differ() {
    let first = DataKey::generate().unwrap();
    let second = DataKey::generate().unwrap();

    assert_ne!(first.expose(), second.expose());
}

proptest! {
    #[test]
    fn any_single_byte_change_is_detected(index in 0usize..4096, flip in 1u8..=255) {
        let mut bytes = sealed(b"some vault contents");
        let at = index % bytes.len();
        bytes[at] ^= flip;

        prop_assert!(open(&bytes, &key(1)).is_err());
    }

    #[test]
    fn opening_random_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = open(&bytes, &key(1));
        let mut prefixed = MAGIC.to_vec();
        prefixed.extend_from_slice(&bytes);
        let _ = open(&prefixed, &key(1));
    }

    #[test]
    fn any_plaintext_survives(plaintext in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let bytes = seal(&header(), &key(4), &plaintext).unwrap();

        prop_assert_eq!(&*open(&bytes, &key(4)).unwrap().1, plaintext.as_slice());
    }
}

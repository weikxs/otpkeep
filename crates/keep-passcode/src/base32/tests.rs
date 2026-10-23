use proptest::prelude::*;

use super::{Base32Fault, decode, encode};

#[test]
fn encodes_rfc_4648_vectors_in_upper_case_without_padding() {
    let cases: [(&[u8], &str); 6] = [
        (b"f", "MY"),
        (b"fo", "MZXQ"),
        (b"foo", "MZXW6"),
        (b"foob", "MZXW6YQ"),
        (b"fooba", "MZXW6YTB"),
        (b"foobar", "MZXW6YTBOI"),
    ];

    for (bytes, text) in cases {
        assert_eq!(*encode(bytes), text);
    }
}

#[test]
fn decodes_rfc_4648_vectors_with_or_without_padding() {
    let cases: [(&str, &[u8]); 6] = [
        ("MY======", b"f"),
        ("MZXQ====", b"fo"),
        ("MZXW6===", b"foo"),
        ("MZXW6YQ=", b"foob"),
        ("MZXW6YTB", b"fooba"),
        ("MZXW6YTBOI======", b"foobar"),
    ];

    for (text, bytes) in cases {
        assert_eq!(*decode(text).unwrap(), bytes, "{text}");
        assert_eq!(
            *decode(text.trim_end_matches('=')).unwrap(),
            bytes,
            "{text}"
        );
    }
}

#[test]
fn decodes_the_rfc_test_secret() {
    assert_eq!(
        *decode("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ").unwrap(),
        b"12345678901234567890"
    );
}

#[test]
fn accepts_the_ways_people_write_secrets() {
    let written = [
        "mzxw6ytboi",
        "MZXW 6YTB OI",
        "mzxw-6ytb-oi",
        "  MZXW6YTBOI\n",
        "MZXW\t6YTB\u{3000}OI",
        "MZXW6YTBOI=",
        "MZXW6YTBOI==========",
    ];

    for text in written {
        assert_eq!(*decode(text).unwrap(), b"foobar", "{text:?}");
    }
}

#[test]
fn reports_the_position_of_an_invalid_character() {
    let cases = [
        ("MZX1", 3, '1'),
        ("MZ XW8", 5, '8'),
        ("0MZXQ", 0, '0'),
        ("MZXQ!", 4, '!'),
        ("字MZXQ", 0, '字'),
    ];

    for (text, at, found) in cases {
        assert_eq!(
            decode(text).err(),
            Some(Base32Fault::InvalidChar { at, found }),
            "{text}"
        );
    }
}

#[test]
fn padding_may_only_come_at_the_end() {
    assert_eq!(
        decode("=MZXQ").err(),
        Some(Base32Fault::MisplacedPadding { at: 0 })
    );
    assert_eq!(
        decode("MZ=XQ").err(),
        Some(Base32Fault::MisplacedPadding { at: 2 })
    );
    assert_eq!(decode("MZXQ= ").map(|b| b.to_vec()), Ok(b"fo".to_vec()));
}

#[test]
fn input_without_data_is_empty() {
    for text in ["", "   ", "====", "- -", "M"] {
        assert_eq!(decode(text).err(), Some(Base32Fault::Empty), "{text:?}");
    }
}

#[test]
fn positions_count_characters_not_bytes() {
    assert_eq!(
        decode("é1").err(),
        Some(Base32Fault::InvalidChar { at: 0, found: 'é' })
    );
    assert_eq!(
        decode("\u{3000}1").err(),
        Some(Base32Fault::InvalidChar { at: 1, found: '1' })
    );
}

proptest! {
    #[test]
    fn decoding_reverses_encoding(bytes in proptest::collection::vec(any::<u8>(), 1..128)) {
        let text = encode(&bytes);

        prop_assert_eq!(&*decode(&text).unwrap(), bytes.as_slice());
        prop_assert_eq!(&*decode(&text.to_lowercase()).unwrap(), bytes.as_slice());
    }

    #[test]
    fn decoding_never_panics(text in "\\PC*") {
        let _ = decode(&text);
    }
}

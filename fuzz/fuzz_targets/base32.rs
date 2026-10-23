#![no_main]
//! Base32 decoding never panics, and decoding a canonical encoding gives back the bytes.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if let Ok(bytes) = keep_passcode::base32::decode(text) {
        let encoded = keep_passcode::base32::encode(&bytes);
        assert_eq!(*keep_passcode::base32::decode(&encoded).expect("canonical Base32 decodes"), *bytes);
    }
});

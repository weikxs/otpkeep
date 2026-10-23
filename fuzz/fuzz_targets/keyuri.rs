#![no_main]
//! otpauth links and link sheets: any text must parse or fail cleanly, and what parses must write and parse back.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if let Ok(parsed) = keep_keyuri::parse_key_uri(text) {
        let written = keep_keyuri::write_key_uri(&parsed.uri);
        let again = keep_keyuri::parse_key_uri(&written).expect("a written link must parse");
        assert_eq!(again.uri.secret, parsed.uri.secret);
    }
    let _ = keep_keyuri::read_sheet(text);
});

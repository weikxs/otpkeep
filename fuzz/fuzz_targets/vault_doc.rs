#![no_main]
//! Decrypted vault and history documents: whatever a damaged or hostile file holds must be rejected cleanly.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(doc) = keep_vault::decode_vault(data) {
        let again = keep_vault::decode_vault(&keep_vault::encode_vault(&doc)).expect("re-encoded vault decodes");
        assert_eq!(again, doc);
    }
    let _ = keep_vault::history::decode_local(data);
});

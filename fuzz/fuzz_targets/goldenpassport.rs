#![no_main]
//! GoldenPassport files from disk: binary plists and JSON.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = keep_transfer::goldenpassport::read_secrets(data);
    let _ = keep_transfer::goldenpassport::read_accounts_json(data);
});

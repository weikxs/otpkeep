#![no_main]
//! Google Authenticator transfer codes: as a link, and as raw payload bytes wrapped into a link.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = keep_transfer::migration::decode_migration_link(&String::from_utf8_lossy(data));
    let hex: String = data.iter().map(|b| format!("%{b:02X}")).collect();
    let _ = keep_transfer::migration::decode_migration_link(&format!("otpauth-migration://offline?data={hex}"));
});

#![no_main]
//! Sealed file headers are read before authentication, so they must survive anything.
use libfuzzer_sys::fuzz_target;
use keep_vault::sealed::{DataKey, open, read_header};

fuzz_target!(|data: &[u8]| {
    let _ = read_header(data);
    let _ = open(data, &DataKey::from_bytes([7; 32]));
});

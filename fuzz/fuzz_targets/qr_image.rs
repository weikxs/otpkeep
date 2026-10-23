#![no_main]
//! Images given to the QR reader, and raw grey-scale frames.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = keep_transfer::qr::decode_qr_image(data);
    if data.len() >= 2 {
        let width = u32::from(data[0]).max(1);
        let height = u32::try_from(data.len() - 1).unwrap_or(0) / width;
        let pixels = &data[1..1 + (width * height) as usize];
        let _ = keep_transfer::qr::decode_qr_luma(width, height, pixels);
    }
});

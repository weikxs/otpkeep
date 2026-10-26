//! Reading a QR code from a large photo or screenshot. Measured on release builds only (`cargo test --release`);
//! debug builds are not representative.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "a test harness that reports its timings"
)]

use std::io::Cursor;
use std::time::{Duration, Instant};

use image::{GrayImage, ImageFormat, Luma};
use keep_transfer::qr::decode_qr_image;
use qrcode::{Color, QrCode};

const LINK: &str =
    "otpauth://totp/Example%20Cloud:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud";
/// A 12-megapixel photo, as phone cameras take them.
const PHOTO_WIDTH: u32 = 4000;
const PHOTO_HEIGHT: u32 = 3000;
/// Pixels per module of the code in the photo: it fills about a third of the picture.
const PHOTO_SCALE: u32 = 30;
/// The import waits on this; it must not feel stuck.
const BUDGET: Duration = Duration::from_secs(1);
const RUNS: usize = 3;

/// A grey photo with `text` as a QR code in its middle.
fn photo(text: &str) -> Vec<u8> {
    let code = QrCode::new(text.as_bytes()).unwrap();
    let width = u32::try_from(code.width()).unwrap();
    let side = width * PHOTO_SCALE;
    let (left, top) = ((PHOTO_WIDTH - side) / 2, (PHOTO_HEIGHT - side) / 2);
    let mut canvas = GrayImage::from_pixel(PHOTO_WIDTH, PHOTO_HEIGHT, Luma([230]));
    for (i, color) in code.to_colors().into_iter().enumerate() {
        if color != Color::Dark {
            continue;
        }
        let i = u32::try_from(i).unwrap();
        let (mx, my) = (i % width, i / width);
        for dy in 0..PHOTO_SCALE {
            for dx in 0..PHOTO_SCALE {
                canvas.put_pixel(
                    left + mx * PHOTO_SCALE + dx,
                    top + my * PHOTO_SCALE + dy,
                    Luma([20]),
                );
            }
        }
    }
    let mut bytes = Vec::new();
    canvas
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

#[cfg_attr(
    debug_assertions,
    ignore = "timing is only meaningful in release builds"
)]
#[test]
fn reads_a_code_from_a_12_megapixel_photo_within_a_second() {
    let bytes = photo(LINK);
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            let found = decode_qr_image(&bytes).unwrap();
            let took = start.elapsed();
            assert_eq!(found.len(), 1);
            assert_eq!(*found[0], LINK);
            took
        })
        .collect();
    times.sort();
    let took = times[RUNS / 2];

    println!("12 MP photo: {took:?}");
    assert!(took <= BUDGET, "took {took:?}");
}

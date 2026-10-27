use std::io::Cursor;

use image::{GrayImage, ImageFormat, Luma};
use qrcode::{Color, QrCode};

use super::{
    MAX_IMAGE_SIDE, QrEcc, QrError, SCAN_SIDE, decode_qr_image, decode_qr_luma, qr_svg, scan_size,
};

const LINK: &str =
    "otpauth://totp/Example%20Cloud:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud";

/// Draws `text` as a QR code: `scale` pixels per module, a 4-module quiet zone, at `x_offset`.
fn draw(canvas: &mut GrayImage, text: &str, scale: u32, x_offset: u32) {
    let code = QrCode::new(text.as_bytes()).unwrap();
    let width = u32::try_from(code.width()).unwrap();
    for (i, color) in code.to_colors().into_iter().enumerate() {
        let (mx, my) = (
            u32::try_from(i).unwrap() % width,
            u32::try_from(i).unwrap() / width,
        );
        if color == Color::Dark {
            for dy in 0..scale {
                for dx in 0..scale {
                    canvas.put_pixel(
                        x_offset + (mx + 4) * scale + dx,
                        (my + 4) * scale + dy,
                        Luma([0]),
                    );
                }
            }
        }
    }
}

pub(crate) fn picture(texts: &[&str], scale: u32) -> GrayImage {
    // Each code gets a square slot as wide as its modules plus the 4-module quiet zone on both sides.
    let widths: Vec<u32> = texts
        .iter()
        .map(|t| u32::try_from(QrCode::new(t.as_bytes()).unwrap().width()).unwrap() + 8)
        .collect();
    let side = widths.iter().max().copied().unwrap_or(0) * scale;
    let mut canvas = GrayImage::from_pixel(
        side * u32::try_from(texts.len()).unwrap(),
        side,
        Luma([255]),
    );
    for (i, text) in texts.iter().enumerate() {
        draw(&mut canvas, text, scale, u32::try_from(i).unwrap() * side);
    }
    canvas
}

pub(crate) fn png(image: &GrayImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn reads_a_code_from_raw_pixels() {
    let image = picture(&[LINK], 4);

    let found = decode_qr_luma(image.width(), image.height(), image.as_raw());

    assert_eq!(found.len(), 1);
    assert_eq!(*found[0], LINK);
}

#[test]
fn reads_every_code_in_a_screenshot() {
    let found = decode_qr_image(&png(&picture(
        &[LINK, "otpauth://totp/b?secret=GEZDGNBVGY3TQOJQ"],
        3,
    )))
    .unwrap();

    let texts: Vec<&str> = found.iter().map(|t| t.as_str()).collect();
    assert_eq!(texts.len(), 2);
    assert!(texts.contains(&LINK));
}

#[test]
fn an_image_without_a_code_finds_nothing() {
    let blank = GrayImage::from_pixel(200, 200, Luma([255]));

    assert!(decode_qr_image(&png(&blank)).unwrap().is_empty());
}

#[test]
fn refuses_what_is_not_an_image() {
    assert_eq!(
        decode_qr_image(b"not an image").err(),
        Some(QrError::NotAnImage)
    );
}

#[test]
fn refuses_images_too_large_to_decode_safely() {
    let wide = GrayImage::from_pixel(MAX_IMAGE_SIDE + 1, 1, Luma([255]));

    assert_eq!(decode_qr_image(&png(&wide)).err(), Some(QrError::TooLarge));
}

#[test]
fn makes_svg_codes_that_read_back() {
    let svg = qr_svg(LINK, QrEcc::Medium).unwrap();

    assert!(
        svg.starts_with("<?xml") || svg.starts_with("<svg"),
        "{}",
        &svg[..40]
    );
    assert!(svg.contains("</svg>"));
    // The SVG must not carry the text in readable form (it is drawn as modules only).
    assert!(!svg.contains("JBSWY3DPEHPK3PXP"));
}

#[test]
fn refuses_text_too_long_for_one_code() {
    assert_eq!(
        qr_svg(&"A".repeat(8_000), QrEcc::Low).err(),
        Some(QrError::TooLong)
    );
}

#[test]
fn a_google_export_survives_being_drawn_and_scanned() {
    use keep_passcode::{CodeSpec, SecretKey};

    use crate::migration::{
        BATCH_BUDGET_BYTES, ExportItem, MigrationCollector, decode_migration_link, encode_migration,
    };
    let secrets: Vec<SecretKey> = (1..=25u8)
        .map(|b| SecretKey::from_bytes(vec![b; 20]).unwrap())
        .collect();
    let accounts: Vec<String> = (1..=25).map(|i| format!("alex{i}@example.com")).collect();
    let items: Vec<ExportItem<'_>> = secrets
        .iter()
        .zip(&accounts)
        .map(|(secret, account)| ExportItem {
            secret,
            spec: CodeSpec::default(),
            counter: 0,
            issuer: "Example Cloud",
            account,
        })
        .collect();
    let export = encode_migration(&items, BATCH_BUDGET_BYTES).unwrap();

    let mut collector = MigrationCollector::default();
    for link in export.links.iter().rev() {
        let scanned = decode_qr_image(&png(&picture(&[link], 3))).unwrap();
        collector = collector
            .with(decode_migration_link(&scanned[0]).unwrap())
            .0;
    }

    let status = &collector.groups()[0];
    assert!(status.complete, "{status:?}");
    let back = collector.take(status.batch_id).unwrap();
    assert_eq!(back.len(), 25);
    assert_eq!(back[24].account, "alex25@example.com");
}

/// A frame found by fuzzing that made rqrr's perspective mapping assert (upstream issue #54); in a release build
/// that would abort the app. Fixed by the patched copy in `vendor/rqrr`.
#[test]
fn a_degenerate_frame_does_not_crash_the_detector() {
    let data = include_bytes!("rqrr-issue-54.bin");
    let width = u32::from(data[0]).max(1);
    let height = u32::try_from(data.len() - 1).unwrap() / width;
    let pixels = &data[1..=usize::try_from(width * height).unwrap()];

    let _ = decode_qr_luma(width, height, pixels);
}

#[test]
fn a_large_picture_is_searched_at_a_reduced_size_first() {
    assert_eq!(scan_size(SCAN_SIDE, 900), None);
    assert_eq!(scan_size(4000, 3000), Some((SCAN_SIDE, 1200)));
    assert_eq!(scan_size(3000, 4000), Some((1200, SCAN_SIDE)));
    assert_eq!(scan_size(MAX_IMAGE_SIDE, 1), Some((SCAN_SIDE, 1)));
}

#[test]
fn reads_a_code_from_a_picture_larger_than_the_reduced_size() {
    // About 2200 pixels wide: read from the reduced copy.
    let image = picture(&[LINK], 60);
    assert!(image.width() > SCAN_SIDE);

    let found = decode_qr_image(&png(&image)).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(*found[0], LINK);
}

#[test]
fn a_small_code_in_a_large_picture_is_found_in_the_full_picture() {
    // Two pixels per module in a 4000-pixel-wide screenshot: too small to survive the reduction.
    let code = picture(&[LINK], 2);
    let mut canvas = GrayImage::from_pixel(4000, code.height(), Luma([255]));
    image::imageops::replace(&mut canvas, &code, 0, 0);

    let found = decode_qr_image(&png(&canvas)).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(*found[0], LINK);
}

#[test]
fn every_code_of_a_large_screenshot_is_read() {
    let texts = [LINK, "otpauth://totp/b?secret=GEZDGNBVGY3TQOJQ"];
    let image = picture(&texts, 30);
    assert!(image.width() > SCAN_SIDE);

    let found = decode_qr_image(&png(&image)).unwrap();

    let found: Vec<&str> = found.iter().map(|t| t.as_str()).collect();
    assert_eq!(found, texts);
}

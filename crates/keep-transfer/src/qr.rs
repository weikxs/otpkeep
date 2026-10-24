//! QR codes: reading them from images and drawing them as SVG (design/05-transfer.md §8).
//!
//! Images are decoded in memory only. Their size is checked from the header before any pixels are decoded, so a
//! small file that claims huge dimensions cannot exhaust memory.

use std::io::Cursor;

use qrcode::{EcLevel, QrCode};
use zeroize::Zeroizing;

/// Widest or tallest image that is decoded.
pub const MAX_IMAGE_SIDE: u32 = 10_000;
/// Most pixels an image may have.
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
/// Smallest side of a drawn code, in SVG units.
const SVG_MIN_SIDE: u32 = 256;

/// Error correction of a drawn code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrEcc {
    /// About 7% recoverable: the most data per code (transfer codes).
    Low,
    /// About 15% recoverable (single links).
    Medium,
}

/// Why a code could not be read or drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrError {
    /// The bytes are not a PNG, JPEG or WebP image.
    #[error("this is not a supported image")]
    NotAnImage,
    /// The image is too large to decode safely.
    #[error("the image is too large")]
    TooLarge,
    /// The text does not fit in one QR code.
    #[error("the text is too long for one QR code")]
    TooLong,
}

/// Every QR code in an image, top to bottom, left to right.
///
/// # Errors
///
/// Returns [`QrError::NotAnImage`] or [`QrError::TooLarge`].
pub fn decode_qr_image(bytes: &[u8]) -> Result<Vec<Zeroizing<String>>, QrError> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| QrError::NotAnImage)?;
    if reader.format().is_none() {
        return Err(QrError::NotAnImage);
    }
    let (width, height) = reader.into_dimensions().map_err(|_| QrError::NotAnImage)?;
    if width > MAX_IMAGE_SIDE
        || height > MAX_IMAGE_SIDE
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err(QrError::TooLarge);
    }
    let luma = image::load_from_memory(bytes)
        .map_err(|_| QrError::NotAnImage)?
        .to_luma8();
    Ok(decode_qr_luma(width, height, luma.as_raw()))
}

/// Every QR code in a grey-scale frame (one byte per pixel, row by row), for example from a camera or a screen
/// capture. A frame whose size does not match yields nothing.
#[must_use]
pub fn decode_qr_luma(width: u32, height: u32, luma: &[u8]) -> Vec<Zeroizing<String>> {
    let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
        return Vec::new();
    };
    if w.checked_mul(h) != Some(luma.len()) {
        return Vec::new();
    }
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| luma[y * w + x]);
    let mut grids = prepared.detect_grids();
    grids.sort_by_key(|grid| {
        let top_left = grid
            .bounds
            .iter()
            .fold((i32::MAX, i32::MAX), |(x, y), p| (x.min(p.x), y.min(p.y)));
        (top_left.1, top_left.0)
    });
    grids
        .iter()
        .filter_map(|grid| grid.decode().ok().map(|(_, text)| Zeroizing::new(text)))
        .collect()
}

/// Draws `text` as an SVG QR code with a quiet zone. The SVG holds the code's modules only, so it is as secret as
/// the text and is wiped when dropped.
///
/// # Errors
///
/// Returns [`QrError::TooLong`] when the text does not fit in one code.
pub fn qr_svg(text: &str, ecc: QrEcc) -> Result<Zeroizing<String>, QrError> {
    let level = match ecc {
        QrEcc::Low => EcLevel::L,
        QrEcc::Medium => EcLevel::M,
    };
    let code = QrCode::with_error_correction_level(text.as_bytes(), level)
        .map_err(|_| QrError::TooLong)?;
    Ok(Zeroizing::new(
        code.render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(SVG_MIN_SIDE, SVG_MIN_SIDE)
            .quiet_zone(true)
            .build(),
    ))
}

#[cfg(test)]
pub(crate) mod tests;

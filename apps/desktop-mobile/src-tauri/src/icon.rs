//! The menu bar icon, drawn here: a rounded shield with a keyhole (design/12 §1). It is a template image, so
//! the system tints it for light and dark menu bars.

/// Side length in pixels: 22 points at 2×.
pub const SIZE: u32 = 44;
/// Samples per pixel side, for smooth edges.
const SUPERSAMPLE: u32 = 4;

/// RGBA pixels, black with coverage in alpha.
#[must_use]
pub fn tray_pixels() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let samples = f64::from(SUPERSAMPLE * SUPERSAMPLE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut covered = 0u32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let u = (f64::from(x * SUPERSAMPLE + sx) + 0.5) / f64::from(SIZE * SUPERSAMPLE);
                    let v = (f64::from(y * SUPERSAMPLE + sy) + 0.5) / f64::from(SIZE * SUPERSAMPLE);
                    if in_shield(u, v) && !in_keyhole(u, v) {
                        covered += 1;
                    }
                }
            }
            let alpha = (f64::from(covered) / samples * 255.0).round();
            pixels.extend_from_slice(&[0, 0, 0, alpha_byte(alpha)]);
        }
    }
    pixels
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is rounded and within 0..=255"
)]
fn alpha_byte(alpha: f64) -> u8 {
    alpha.clamp(0.0, 255.0) as u8
}

const TOP: f64 = 0.08;
const SHOULDER: f64 = 0.52;
const BOTTOM: f64 = 0.94;
const HALF_WIDTH: f64 = 0.34;
const CORNER: f64 = 0.1;

/// The shield: straight sides from the top to the shoulder, then curving in to a point.
fn in_shield(u: f64, v: f64) -> bool {
    if !(TOP..=BOTTOM).contains(&v) {
        return false;
    }
    let dx = (u - 0.5).abs();
    let half = if v <= SHOULDER {
        HALF_WIDTH
    } else {
        let t = (v - SHOULDER) / (BOTTOM - SHOULDER);
        HALF_WIDTH * (1.0 - t * t).sqrt() * (1.0 - 0.35 * t)
    };
    if dx > half {
        return false;
    }
    // Rounded top corners.
    let corner_x = HALF_WIDTH - CORNER;
    let corner_y = TOP + CORNER;
    if dx > corner_x && v < corner_y {
        return (dx - corner_x).hypot(v - corner_y) <= CORNER;
    }
    true
}

const HOLE_X: f64 = 0.5;
const HOLE_Y: f64 = 0.42;
const HOLE_R: f64 = 0.095;
const SLOT_BOTTOM: f64 = 0.7;
const SLOT_TOP_HALF: f64 = 0.035;
const SLOT_BOTTOM_HALF: f64 = 0.065;

/// The keyhole: a circle over a slot that widens downwards.
fn in_keyhole(u: f64, v: f64) -> bool {
    if (u - HOLE_X).hypot(v - HOLE_Y) <= HOLE_R {
        return true;
    }
    if !(HOLE_Y..=SLOT_BOTTOM).contains(&v) {
        return false;
    }
    let t = (v - HOLE_Y) / (SLOT_BOTTOM - HOLE_Y);
    (u - HOLE_X).abs() <= SLOT_TOP_HALF + (SLOT_BOTTOM_HALF - SLOT_TOP_HALF) * t
}

#[cfg(test)]
mod tests {
    use super::{SIZE, tray_pixels};

    fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
        pixels[((y * SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn has_one_pixel_per_point_at_2x() {
        assert_eq!(tray_pixels().len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn is_solid_shield_with_a_clear_keyhole() {
        let pixels = tray_pixels();

        assert_eq!(alpha(&pixels, 0, 0), 0, "outside");
        assert_eq!(alpha(&pixels, SIZE / 2, SIZE * 42 / 100), 0, "keyhole");
        assert_eq!(alpha(&pixels, SIZE / 4, SIZE / 3), 255, "shield body");
        assert_eq!(alpha(&pixels, SIZE / 2, SIZE - 1), 0, "below the point");
    }

    #[test]
    fn is_black_so_the_system_can_tint_it() {
        let pixels = tray_pixels();

        assert!(pixels.chunks(4).all(|p| p[..3] == [0, 0, 0]));
    }
}

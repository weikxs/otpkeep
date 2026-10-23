# rqrr 0.11.0, patched

Copied unchanged from crates.io `rqrr` 0.11.0 (licenses: `LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-ISC`),
except one change in `src/geometry.rs`, `Perspective::map`:

- the four `assert!`s that panic when a degenerate perspective maps a point far outside the image (or to NaN) are
  replaced by clamping to the `i32` range. Every caller already bounds-checks the point before reading a pixel.

Upstream issue: https://github.com/WanzenBug/rqrr/issues/54 (open). OTPKeep's fuzzing found an image that
triggers it; in a release build, where panics abort, it would close the app. Remove this copy and the
`[patch.crates-io]` entries once upstream ships a fix.

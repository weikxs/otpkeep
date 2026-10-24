//! HOTP and TOTP passcodes (decimal and Steam Guard), Base32 and code specifications.

pub mod base32;
mod passcode;
mod secret;
mod spec;

pub use passcode::{Moment, NEXT_CODE_WINDOW_S, Passcode, PasscodeError, derive_passcode};
pub use secret::{SHORT_SECRET_BYTES, SecretError, SecretKey};
pub use spec::{
    CodeKind, CodeSpec, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, Encoding, MAX_DIGITS,
    MAX_PERIOD_S, MIN_DIGITS, MIN_PERIOD_S, STEAM_DIGITS, SpecError,
};

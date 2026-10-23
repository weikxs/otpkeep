//! HOTP and TOTP passcodes, Base32 and code specifications.

mod passcode;
mod secret;
mod spec;

pub use passcode::{Moment, NEXT_CODE_WINDOW_S, Passcode, PasscodeError, derive_passcode};
pub use secret::{SHORT_SECRET_BYTES, SecretError, SecretKey};
pub use spec::{
    CodeKind, CodeSpec, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, MAX_DIGITS, MAX_PERIOD_S,
    MIN_DIGITS, MIN_PERIOD_S, SpecError,
};

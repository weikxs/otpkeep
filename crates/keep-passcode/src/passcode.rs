//! Deriving HOTP (RFC 4226) and TOTP (RFC 6238) passcodes.

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};

use crate::{CodeKind, CodeSpec, Digest, Encoding, SecretKey};

/// In the last this-many seconds of a TOTP period, the next code is offered as well (PRD FR-1.4).
pub const NEXT_CODE_WINDOW_S: u16 = 5;

/// Bytes read by RFC 4226 dynamic truncation.
const TRUNCATED_LEN: usize = 4;
/// The characters of Steam Guard codes; the truncated value is written in base 26, lowest place first.
const STEAM_ALPHABET: &[u8; 26] = b"23456789BCDFGHJKMNPQRTVWXY";

/// The point in "code time" to derive a passcode for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// Unix time in seconds, for TOTP. Times before 1970 count as 1970.
    Time(i64),
    /// The counter value, for HOTP.
    Counter(u64),
}

/// Why a passcode could not be derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasscodeError {
    /// The algorithm cannot produce well-defined passcodes (see [`Digest::can_compute`]).
    #[error("passcodes cannot be computed with {0:?}")]
    UnsupportedDigest(Digest),
    /// A time was given for an HOTP entry, or a counter for a TOTP entry.
    #[error("the moment does not match the kind of passcode")]
    MomentMismatch,
}

/// A derived passcode. `Debug` never shows the digits.
#[derive(Clone, PartialEq, Eq)]
pub struct Passcode {
    code: String,
    valid_for_s: Option<u16>,
    next: Option<String>,
}

impl Passcode {
    /// The digits, with leading zeros.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Seconds until a TOTP code expires; `None` for HOTP.
    #[must_use]
    pub const fn valid_for_s(&self) -> Option<u16> {
        self.valid_for_s
    }

    /// The following TOTP code, offered in the last [`NEXT_CODE_WINDOW_S`] seconds of a period.
    #[must_use]
    pub fn next(&self) -> Option<&str> {
        self.next.as_deref()
    }
}

impl fmt::Debug for Passcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Passcode")
            .field("code", &"redacted")
            .field("valid_for_s", &self.valid_for_s)
            .field("has_next", &self.next.is_some())
            .finish()
    }
}

/// Derives the passcode of `secret` under `spec` at `moment`.
///
/// # Errors
///
/// - [`PasscodeError::UnsupportedDigest`] for MD5.
/// - [`PasscodeError::MomentMismatch`] when `moment` does not fit the kind of code.
pub fn derive_passcode(
    secret: &SecretKey,
    spec: CodeSpec,
    moment: Moment,
) -> Result<Passcode, PasscodeError> {
    if !spec.digest().can_compute() {
        return Err(PasscodeError::UnsupportedDigest(spec.digest()));
    }
    match (spec.kind(), moment) {
        (CodeKind::CounterBased, Moment::Counter(counter)) => Ok(Passcode {
            code: counter_code(secret, spec, counter)?,
            valid_for_s: None,
            next: None,
        }),
        (CodeKind::TimeBased { period_s }, Moment::Time(unix_s)) => {
            time_passcode(secret, spec, period_s, unix_s)
        }
        _ => Err(PasscodeError::MomentMismatch),
    }
}

fn time_passcode(
    secret: &SecretKey,
    spec: CodeSpec,
    period_s: u16,
    unix_s: i64,
) -> Result<Passcode, PasscodeError> {
    let seconds = u64::try_from(unix_s).unwrap_or(0);
    let period = u64::from(period_s);
    let step = seconds / period;
    // The remainder is below the period, which is a u16, so the conversion cannot fail.
    let elapsed = u16::try_from(seconds % period).unwrap_or(0);
    let valid_for_s = period_s - elapsed;
    let next = if valid_for_s <= NEXT_CODE_WINDOW_S {
        Some(counter_code(secret, spec, step.saturating_add(1))?)
    } else {
        None
    };
    Ok(Passcode {
        code: counter_code(secret, spec, step)?,
        valid_for_s: Some(valid_for_s),
        next,
    })
}

/// The passcode for one counter value, written as the specification says.
fn counter_code(secret: &SecretKey, spec: CodeSpec, counter: u64) -> Result<String, PasscodeError> {
    let value = truncated(secret, spec, counter)?;
    Ok(match spec.encoding() {
        Encoding::Decimal => decimal(value, spec.digits()),
        Encoding::Steam => steam(value, spec.digits()),
    })
}

/// RFC 4226: modulo `10^digits`, zero padded.
fn decimal(value: u32, digits: u8) -> String {
    let modulus = 10u32.pow(u32::from(digits));
    let width = usize::from(digits);
    format!("{:0width$}", value % modulus)
}

/// Steam Guard: base 26 over [`STEAM_ALPHABET`], lowest place first.
fn steam(mut value: u32, digits: u8) -> String {
    let base = u32::try_from(STEAM_ALPHABET.len()).unwrap_or(u32::MAX);
    (0..digits)
        .map(|_| {
            let index = usize::try_from(value % base).unwrap_or(0);
            value /= base;
            char::from(STEAM_ALPHABET[index])
        })
        .collect()
}

/// HMAC and RFC 4226 dynamic truncation: a 31-bit value.
fn truncated(secret: &SecretKey, spec: CodeSpec, counter: u64) -> Result<u32, PasscodeError> {
    let mac = hmac_of(spec.digest(), secret.expose(), &counter.to_be_bytes())?;
    let last = mac.last().copied().unwrap_or(0);
    let offset = usize::from(last & 0x0f);
    let window = mac
        .get(offset..offset + TRUNCATED_LEN)
        .ok_or(PasscodeError::UnsupportedDigest(spec.digest()))?;
    let mut word = [0u8; TRUNCATED_LEN];
    word.copy_from_slice(window);
    Ok(u32::from_be_bytes(word) & 0x7fff_ffff)
}

fn hmac_of(digest: Digest, key: &[u8], message: &[u8]) -> Result<Vec<u8>, PasscodeError> {
    fn run<M: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Vec<u8> {
        // HMAC accepts keys of any length, so this never fails.
        match <M as KeyInit>::new_from_slice(key) {
            Ok(mut mac) => {
                mac.update(message);
                mac.finalize().into_bytes().to_vec()
            }
            Err(_) => Vec::new(),
        }
    }
    match digest {
        Digest::Sha1 => Ok(run::<Hmac<sha1::Sha1>>(key, message)),
        Digest::Sha256 => Ok(run::<Hmac<sha2::Sha256>>(key, message)),
        Digest::Sha512 => Ok(run::<Hmac<sha2::Sha512>>(key, message)),
        Digest::Md5 => Err(PasscodeError::UnsupportedDigest(digest)),
    }
}

#[cfg(test)]
mod tests;

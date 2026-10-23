//! How a passcode is computed: kind, hash algorithm and number of digits.

/// Fewest digits a passcode may have.
pub const MIN_DIGITS: u8 = 6;
/// Most digits a passcode may have.
pub const MAX_DIGITS: u8 = 8;
/// Digits used when a link or form does not say.
pub const DEFAULT_DIGITS: u8 = 6;
/// Shortest allowed TOTP period.
pub const MIN_PERIOD_S: u16 = 1;
/// Longest allowed TOTP period.
pub const MAX_PERIOD_S: u16 = 300;
/// TOTP period used when a link or form does not say.
pub const DEFAULT_PERIOD_S: u16 = 30;

/// Whether codes follow the clock (TOTP) or a counter (HOTP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeKind {
    /// TOTP: a new code every `period_s` seconds.
    TimeBased {
        /// Length of one time step in seconds.
        period_s: u16,
    },
    /// HOTP: a new code each time the counter moves on.
    CounterBased,
}

/// The hash algorithm behind the HMAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Digest {
    /// SHA-1, the default of almost every service.
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
    /// MD5. Kept only so imported entries are not lost.
    Md5,
}

impl Digest {
    /// Whether passcodes can be computed with this algorithm.
    ///
    /// MD5 cannot: RFC 4226 truncation reads four bytes at an offset of up to 15, which needs a digest of at
    /// least 19 bytes, and MD5 has 16. The result is undefined, so OTPKeep refuses rather than guess.
    #[must_use]
    pub const fn can_compute(self) -> bool {
        !matches!(self, Self::Md5)
    }
}

/// Why a code specification is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    /// The number of digits is outside 6–8.
    #[error("a passcode has {MIN_DIGITS} to {MAX_DIGITS} digits, not {0}")]
    DigitsOutOfRange(u8),
    /// The TOTP period is outside 1–300 seconds.
    #[error("a period is {MIN_PERIOD_S} to {MAX_PERIOD_S} seconds, not {0}")]
    PeriodOutOfRange(u16),
}

/// A validated description of how to compute an entry's passcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CodeSpec {
    kind: CodeKind,
    digest: Digest,
    digits: u8,
}

impl CodeSpec {
    /// Validates and builds a specification.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError`] when the digits or the period are out of range.
    pub fn new(kind: CodeKind, digest: Digest, digits: u8) -> Result<Self, SpecError> {
        if !(MIN_DIGITS..=MAX_DIGITS).contains(&digits) {
            return Err(SpecError::DigitsOutOfRange(digits));
        }
        if let CodeKind::TimeBased { period_s } = kind
            && !(MIN_PERIOD_S..=MAX_PERIOD_S).contains(&period_s)
        {
            return Err(SpecError::PeriodOutOfRange(period_s));
        }
        Ok(Self {
            kind,
            digest,
            digits,
        })
    }

    /// TOTP or HOTP.
    #[must_use]
    pub const fn kind(&self) -> CodeKind {
        self.kind
    }

    /// The hash algorithm.
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// The number of digits in each passcode.
    #[must_use]
    pub const fn digits(&self) -> u8 {
        self.digits
    }
}

impl Default for CodeSpec {
    /// TOTP, 30 seconds, SHA-1, 6 digits: what a link means when it says nothing else.
    fn default() -> Self {
        Self {
            kind: CodeKind::TimeBased {
                period_s: DEFAULT_PERIOD_S,
            },
            digest: Digest::Sha1,
            digits: DEFAULT_DIGITS,
        }
    }
}

#[cfg(test)]
mod tests;

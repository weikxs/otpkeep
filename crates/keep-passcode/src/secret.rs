//! The raw bytes of a two-factor secret.

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

/// Secrets shorter than this many bytes are accepted but flagged as weak.
pub const SHORT_SECRET_BYTES: usize = 10;

/// Why a secret could not be created.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// The secret has no bytes.
    #[error("the secret is empty")]
    Empty,
}

/// The raw bytes of a two-factor secret.
///
/// The bytes are wiped from memory when the value is dropped, and `Debug` never shows them.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey {
    bytes: Vec<u8>,
}

impl SecretKey {
    /// Wraps the raw secret bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Empty`] when `bytes` is empty.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, SecretError> {
        if bytes.is_empty() {
            return Err(SecretError::Empty);
        }
        Ok(Self { bytes })
    }

    /// The raw bytes. Callers must not log, display or persist them unencrypted.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the secret is shorter than [`SHORT_SECRET_BYTES`] and therefore weak.
    #[must_use]
    pub fn is_short(&self) -> bool {
        self.bytes.len() < SHORT_SECRET_BYTES
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(redacted)")
    }
}

#[cfg(test)]
mod tests;

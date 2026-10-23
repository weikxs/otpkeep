//! What can be wrong with a link, and what is worth telling the user even when it parses.

/// Why a text is not a usable `otpauth://` link.
///
/// Messages never repeat the secret; [`KeyUriFault::SecretInvalid`] names only the one offending character.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyUriFault {
    /// The text is not a link at all.
    #[error("this is not an otpauth:// link")]
    NotAUri,
    /// A link, but not an `otpauth` one.
    #[error("links starting with '{found}:' are not two-factor links")]
    WrongScheme {
        /// The scheme that was found.
        found: String,
    },
    /// Neither `totp` nor `hotp`.
    #[error("'{found}' codes are not supported; only totp and hotp are")]
    UnsupportedKind {
        /// The kind that was found.
        found: String,
    },
    /// No secret, or an empty one.
    #[error("the link has no secret")]
    SecretMissing,
    /// The secret is not valid Base32.
    #[error("the secret has '{found}' at position {at}, which is not allowed in Base32")]
    SecretInvalid {
        /// Character position within the secret value, after percent-decoding.
        at: usize,
        /// The offending character.
        found: char,
    },
    /// An `algorithm` OTPKeep does not know.
    #[error("the algorithm '{found}' is not supported")]
    UnsupportedAlgorithm {
        /// The value that was found.
        found: String,
    },
    /// `digits`, `period` or `counter` is not a number in range.
    #[error("'{value}' is not a valid {name}")]
    ParamOutOfRange {
        /// The parameter name.
        name: &'static str,
        /// The value that was found.
        value: String,
    },
    /// An HOTP link without a counter.
    #[error("an hotp link needs a counter")]
    CounterMissing,
}

/// Things worth telling the user about a link that did parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyUriNote {
    /// The secret is shorter than 10 bytes, which is weak.
    ShortSecret,
    /// A parameter appeared more than once; the first value was used.
    DuplicateParam {
        /// The parameter name, in lower case.
        name: String,
    },
    /// The label names a different issuer than the `issuer` parameter, which was used.
    IssuerMismatch {
        /// The issuer written in the label.
        in_label: String,
    },
}

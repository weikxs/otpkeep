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
    /// A Google Authenticator export (`otpauth-migration://`): it holds several accounts and is imported, not
    /// added.
    #[error("this is a Google Authenticator export; import it instead of adding it")]
    GoogleExport,
    /// A Microsoft Authenticator sign-in approval code (`phonefactor://`). It carries no secret; on the Microsoft
    /// set-up page, choose to use a different authenticator app to get an ordinary code.
    #[error(
        "this is a Microsoft Authenticator approval code, which only that app can use; choose 'use a different authenticator app' when setting up to get an ordinary code"
    )]
    MicrosoftPush,
    /// An Oracle Mobile Authenticator enrolment code (`oraclemobileauthenticator://`). Its secret can only be
    /// unlocked by Oracle's server; an administrator can provide an ordinary TOTP code instead.
    #[error(
        "this is an Oracle Mobile Authenticator enrolment code, which only that app can use; ask for an ordinary TOTP code instead"
    )]
    OracleEnrolment,
    /// Only a secret, without a link around it; add it with the form, which asks for the other details.
    #[error("this looks like a secret on its own; add it with the form instead")]
    BareSecret,
}

/// Things worth telling the user about a link that did parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyUriNote {
    /// An HOTP link without a counter; counting starts at 0, as other apps do.
    CounterAssumedZero,
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

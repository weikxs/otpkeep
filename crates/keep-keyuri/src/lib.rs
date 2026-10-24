//! Parsing and writing `otpauth://` links (the Key URI format) and link sheets.

use keep_passcode::{CodeSpec, SecretKey};

mod clean;
mod fault;
mod parse;
mod sheet;
mod write;

pub use fault::{KeyUriFault, KeyUriNote};
pub use parse::{ParsedKeyUri, parse_key_uri};
pub use sheet::{
    SheetEntry, SheetItem, SheetProblem, SheetProblemKind, SheetRead, read_sheet, write_sheet,
};
pub use write::write_key_uri;

/// Everything an `otpauth://` link says about one account.
///
/// `Debug` shows no secret: [`SecretKey`] redacts itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUri {
    /// The shared secret.
    pub secret: SecretKey,
    /// Kind, algorithm, digits and period.
    pub spec: CodeSpec,
    /// The HOTP counter; 0 for TOTP.
    pub counter: u64,
    /// The service, possibly empty.
    pub issuer: String,
    /// The account at the service, possibly empty.
    pub account: String,
}

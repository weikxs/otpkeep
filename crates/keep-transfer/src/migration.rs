//! Google Authenticator's "transfer accounts" QR codes: `otpauth-migration://offline?data=<base64 protobuf>`
//! (design/05-transfer.md §5). Only facts about the format are used; no other project's code.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use percent_encoding::percent_decode_str;
use zeroize::Zeroizing;

mod collect;
mod export;
mod wire;

pub use collect::{Added, BatchStatus, MigrationCollector};
pub use export::{
    BATCH_BUDGET_BYTES, ExportError, ExportItem, Incompatible, MigrationExport, encode_migration,
};
use wire::{Reader, Value};

const SCHEME: &str = "otpauth-migration://";

// MigrationPayload fields.
const PAYLOAD_PARAMS: u64 = 1;
const PAYLOAD_VERSION: u64 = 2;
const PAYLOAD_BATCH_SIZE: u64 = 3;
const PAYLOAD_BATCH_INDEX: u64 = 4;
const PAYLOAD_BATCH_ID: u64 = 5;
// OtpParameters fields.
const PARAM_SECRET: u64 = 1;
const PARAM_NAME: u64 = 2;
const PARAM_ISSUER: u64 = 3;
const PARAM_ALGORITHM: u64 = 4;
const PARAM_DIGITS: u64 = 5;
const PARAM_TYPE: u64 = 6;
const PARAM_COUNTER: u64 = 7;

/// Why a text is not a readable migration link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MigrationFault {
    /// Not an `otpauth-migration://` link.
    #[error("this is not a Google Authenticator transfer code")]
    NotAMigrationLink,
    /// The link has no `data` parameter.
    #[error("the transfer code has no data")]
    DataMissing,
    /// The data is not Base64.
    #[error("the transfer code data is not readable")]
    BadBase64,
    /// The data is not a well-formed payload.
    #[error("the transfer code data is damaged")]
    BadPayload,
}

/// Why one account in a batch was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// An algorithm value OTPKeep does not know.
    Algorithm(u64),
    /// A digits value OTPKeep does not know.
    Digits(u64),
    /// A type value OTPKeep does not know.
    Kind(u64),
    /// No secret.
    NoSecret,
    /// The account's data is damaged.
    Malformed,
}

/// A value that was "unspecified" in the code and was read as the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assumed {
    /// Read as SHA-1.
    Algorithm,
    /// Read as 6 digits.
    Digits,
    /// Read as TOTP.
    Kind,
}

/// One account from a transfer code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationItem {
    /// The secret.
    pub secret: SecretKey,
    /// Service name.
    pub issuer: String,
    /// Account name.
    pub account: String,
    /// How codes are computed. Transfer codes cannot express a period; it is always 30 seconds.
    pub spec: CodeSpec,
    /// HOTP counter.
    pub counter: u64,
    /// Values that were unspecified and read as defaults.
    pub assumed: Vec<Assumed>,
}

/// An account that could not be read, by its position in the batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemProblem {
    /// Position in the batch, from 0.
    pub index: usize,
    /// Why it was skipped.
    pub reason: Unsupported,
}

/// One transfer code: one batch of an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationBatch {
    /// Format version written by the exporter.
    pub version: u64,
    /// Shared by all codes of one export.
    pub batch_id: i32,
    /// How many codes the export has.
    pub batch_size: u32,
    /// Which code this is, from 0.
    pub batch_index: u32,
    /// Accounts read.
    pub items: Vec<MigrationItem>,
    /// Accounts skipped.
    pub unreadable: Vec<ItemProblem>,
    /// Size of the payload in bytes.
    pub encoded_len: usize,
}

/// Reads a transfer code as a scanner or a paste returns it.
///
/// # Errors
///
/// Returns [`MigrationFault`] when the link or its payload cannot be read.
pub fn decode_migration_link(text: &str) -> Result<MigrationBatch, MigrationFault> {
    let text = text.trim();
    let prefix = text
        .get(..SCHEME.len())
        .ok_or(MigrationFault::NotAMigrationLink)?;
    if !prefix.eq_ignore_ascii_case(SCHEME) {
        return Err(MigrationFault::NotAMigrationLink);
    }
    let query = text.split_once('?').map_or("", |(_, q)| q);
    let data = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("data="))
        .ok_or(MigrationFault::DataMissing)?;
    // Some scanners turn '+' into a space; Base64 has no spaces, so they can be put back.
    let decoded = Zeroizing::new(
        percent_decode_str(data)
            .decode_utf8_lossy()
            .replace(' ', "+"),
    );
    let unpadded = decoded.trim_end_matches('=');
    let bytes = Zeroizing::new(
        STANDARD_NO_PAD
            .decode(unpadded)
            .or_else(|_| URL_SAFE_NO_PAD.decode(unpadded))
            .map_err(|_| MigrationFault::BadBase64)?,
    );
    decode_payload(&bytes)
}

fn decode_payload(bytes: &[u8]) -> Result<MigrationBatch, MigrationFault> {
    let mut batch = MigrationBatch {
        version: 0,
        batch_id: 0,
        batch_size: 0,
        batch_index: 0,
        items: Vec::new(),
        unreadable: Vec::new(),
        encoded_len: bytes.len(),
    };
    let mut reader = Reader::new(bytes);
    let mut index = 0;
    while let Some((field, value)) = reader
        .next_field()
        .map_err(|_| MigrationFault::BadPayload)?
    {
        match (field, value) {
            (PAYLOAD_PARAMS, Value::Bytes(message)) => {
                match decode_item(message) {
                    Ok(item) => batch.items.push(item),
                    Err(reason) => batch.unreadable.push(ItemProblem { index, reason }),
                }
                index += 1;
            }
            (PAYLOAD_VERSION, Value::Varint(v)) => batch.version = v,
            (PAYLOAD_BATCH_SIZE, Value::Varint(v)) => {
                batch.batch_size = int32(v).try_into().unwrap_or(0);
            }
            (PAYLOAD_BATCH_INDEX, Value::Varint(v)) => {
                batch.batch_index = int32(v).try_into().unwrap_or(0);
            }
            (PAYLOAD_BATCH_ID, Value::Varint(v)) => batch.batch_id = int32(v),
            _ => {}
        }
    }
    Ok(batch)
}

/// The fields of one OtpParameters message, before they are checked.
#[derive(Default)]
struct RawItem {
    secret: Zeroizing<Vec<u8>>,
    name: String,
    issuer: String,
    algorithm: u64,
    digits: u64,
    kind: u64,
    counter: u64,
}

fn decode_item(message: &[u8]) -> Result<MigrationItem, Unsupported> {
    let mut raw = RawItem::default();
    let mut reader = Reader::new(message);
    while let Some((field, value)) = reader.next_field().map_err(|_| Unsupported::Malformed)? {
        match (field, value) {
            (PARAM_SECRET, Value::Bytes(b)) => raw.secret = Zeroizing::new(b.to_vec()),
            (PARAM_NAME, Value::Bytes(b)) => raw.name = String::from_utf8_lossy(b).into_owned(),
            (PARAM_ISSUER, Value::Bytes(b)) => raw.issuer = String::from_utf8_lossy(b).into_owned(),
            (PARAM_ALGORITHM, Value::Varint(v)) => raw.algorithm = v,
            (PARAM_DIGITS, Value::Varint(v)) => raw.digits = v,
            (PARAM_TYPE, Value::Varint(v)) => raw.kind = v,
            (PARAM_COUNTER, Value::Varint(v)) => raw.counter = v,
            _ => {}
        }
    }
    let mut assumed = Vec::new();
    let digest = match raw.algorithm {
        0 => {
            assumed.push(Assumed::Algorithm);
            Digest::Sha1
        }
        1 => Digest::Sha1,
        2 => Digest::Sha256,
        3 => Digest::Sha512,
        4 => Digest::Md5,
        other => return Err(Unsupported::Algorithm(other)),
    };
    let digits = match raw.digits {
        0 => {
            assumed.push(Assumed::Digits);
            6
        }
        1 => 6,
        2 => 8,
        other => return Err(Unsupported::Digits(other)),
    };
    let kind = match raw.kind {
        0 => {
            assumed.push(Assumed::Kind);
            CodeKind::TimeBased { period_s: 30 }
        }
        1 => CodeKind::CounterBased,
        2 => CodeKind::TimeBased { period_s: 30 },
        other => return Err(Unsupported::Kind(other)),
    };
    let spec = CodeSpec::new(kind, digest, digits).map_err(|_| Unsupported::Malformed)?;
    let secret = SecretKey::from_bytes(raw.secret.to_vec()).map_err(|_| Unsupported::NoSecret)?;
    let (issuer, account) = split_name(&raw.name, &raw.issuer);
    let counter = if kind == CodeKind::CounterBased {
        raw.counter
    } else {
        0
    };
    Ok(MigrationItem {
        secret,
        issuer,
        account,
        spec,
        counter,
        assumed,
    })
}

/// `issuer:account` names are split when no issuer is given, and stripped of the issuer prefix when one is.
fn split_name(name: &str, issuer: &str) -> (String, String) {
    let (name, issuer) = (name.trim(), issuer.trim());
    if issuer.is_empty() {
        return match name.split_once(':') {
            Some((issuer, account)) => (issuer.trim().to_owned(), account.trim().to_owned()),
            None => (String::new(), name.to_owned()),
        };
    }
    let account = name
        .strip_prefix(issuer)
        .and_then(|rest| rest.strip_prefix(':'))
        .map_or(name, str::trim);
    (issuer.to_owned(), account.to_owned())
}

/// A protobuf `int32` read from a varint: the low 32 bits, as two's complement.
fn int32(value: u64) -> i32 {
    // Truncation to the low 32 bits is what protobuf specifies for int32.
    #[allow(clippy::cast_possible_truncation)]
    let low = value as u32;
    i32::from_ne_bytes(low.to_ne_bytes())
}

#[cfg(test)]
mod tests;

//! Writing transfer codes for Google Authenticator (design/05-transfer.md §5.4).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use zeroize::Zeroizing;

use super::wire::{put_bytes_field, put_varint_field};
use super::{
    PARAM_ALGORITHM, PARAM_COUNTER, PARAM_DIGITS, PARAM_ISSUER, PARAM_NAME, PARAM_SECRET,
    PARAM_TYPE, PAYLOAD_BATCH_ID, PAYLOAD_BATCH_INDEX, PAYLOAD_BATCH_SIZE, PAYLOAD_PARAMS,
    PAYLOAD_VERSION,
};

/// Payload bytes per transfer code. A starting value, to be calibrated against Google Authenticator on real
/// phones (design/10-reference.md, V1).
pub const BATCH_BUDGET_BYTES: usize = 600;
/// Room for the batch fields (version, size, index, id) around the accounts.
const BATCH_HEADER_BYTES: usize = 24;
const PAYLOAD_VERSION_VALUE: u64 = 1;
const TRANSFER_PERIOD_S: u16 = 30;

/// One account to export.
#[derive(Debug, Clone, Copy)]
pub struct ExportItem<'a> {
    /// The secret.
    pub secret: &'a SecretKey,
    /// How codes are computed.
    pub spec: CodeSpec,
    /// HOTP counter.
    pub counter: u64,
    /// Service name.
    pub issuer: &'a str,
    /// Account name.
    pub account: &'a str,
}

/// Why an account cannot go into a transfer code; it is offered as its own `otpauth://` code instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incompatible {
    /// Transfer codes have no period; Google Authenticator assumes 30 seconds.
    Period(u16),
    /// Transfer codes can express only 6 or 8 digits.
    Digits(u8),
    /// MD5 codes are not well defined (design/03-passcode.md §1).
    Md5,
}

/// The result of an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationExport {
    /// The transfer codes, to show one after another. They contain secrets and are wiped when dropped.
    pub links: Vec<Zeroizing<String>>,
    /// Accounts left out, by input position, with the reason.
    pub separate: Vec<(usize, Incompatible)>,
    /// Accounts exported with SHA-256 or SHA-512, whose support in Google Authenticator is unverified (V2).
    pub algorithm_unverified: Vec<usize>,
}

/// Why the export failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    /// The system random source failed (needed for the batch id).
    #[error("the system random number generator is unavailable")]
    RandomUnavailable,
}

/// Packs the accounts into as few transfer codes as fit `budget` payload bytes each; an account larger than the
/// budget gets a code of its own.
///
/// # Errors
///
/// Returns [`ExportError::RandomUnavailable`] when no batch id can be drawn.
pub fn encode_migration(
    items: &[ExportItem<'_>],
    budget: usize,
) -> Result<MigrationExport, ExportError> {
    let mut separate = Vec::new();
    let mut algorithm_unverified = Vec::new();
    let mut encoded: Vec<Zeroizing<Vec<u8>>> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(reason) = compatibility(item.spec) {
            separate.push((index, reason));
            continue;
        }
        if matches!(item.spec.digest(), Digest::Sha256 | Digest::Sha512) {
            algorithm_unverified.push(index);
        }
        encoded.push(encode_item(item));
    }
    let batches = pack(&encoded, budget.saturating_sub(BATCH_HEADER_BYTES));
    let batch_id = random_batch_id()?;
    let size = batches.len() as u64;
    let links = batches
        .iter()
        .zip(0u64..)
        .map(|(members, index)| {
            let mut payload = Zeroizing::new(Vec::new());
            for &member in members {
                put_bytes_field(&mut payload, PAYLOAD_PARAMS, &encoded[member]);
            }
            put_varint_field(&mut payload, PAYLOAD_VERSION, PAYLOAD_VERSION_VALUE);
            put_varint_field(&mut payload, PAYLOAD_BATCH_SIZE, size);
            put_varint_field(&mut payload, PAYLOAD_BATCH_INDEX, index);
            put_varint_field(&mut payload, PAYLOAD_BATCH_ID, batch_id);
            let data = Zeroizing::new(STANDARD.encode(&*payload));
            Zeroizing::new(format!(
                "otpauth-migration://offline?data={}",
                utf8_percent_encode(&data, NON_ALPHANUMERIC)
            ))
        })
        .collect();
    Ok(MigrationExport {
        links,
        separate,
        algorithm_unverified,
    })
}

fn compatibility(spec: CodeSpec) -> Option<Incompatible> {
    if let CodeKind::TimeBased { period_s } = spec.kind()
        && period_s != TRANSFER_PERIOD_S
    {
        return Some(Incompatible::Period(period_s));
    }
    if !matches!(spec.digits(), 6 | 8) {
        return Some(Incompatible::Digits(spec.digits()));
    }
    (spec.digest() == Digest::Md5).then_some(Incompatible::Md5)
}

fn encode_item(item: &ExportItem<'_>) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::new());
    put_bytes_field(&mut out, PARAM_SECRET, item.secret.expose());
    put_bytes_field(&mut out, PARAM_NAME, name(item).as_bytes());
    put_bytes_field(&mut out, PARAM_ISSUER, item.issuer.as_bytes());
    let algorithm = match item.spec.digest() {
        Digest::Sha1 => 1,
        Digest::Sha256 => 2,
        Digest::Sha512 => 3,
        Digest::Md5 => 4,
    };
    put_varint_field(&mut out, PARAM_ALGORITHM, algorithm);
    put_varint_field(
        &mut out,
        PARAM_DIGITS,
        if item.spec.digits() == 8 { 2 } else { 1 },
    );
    match item.spec.kind() {
        CodeKind::CounterBased => {
            put_varint_field(&mut out, PARAM_TYPE, 1);
            put_varint_field(&mut out, PARAM_COUNTER, item.counter);
        }
        CodeKind::TimeBased { .. } => put_varint_field(&mut out, PARAM_TYPE, 2),
    }
    out
}

/// The account as the name, unless reading it back would split it or strip part of it; then `issuer:account`.
fn name(item: &ExportItem<'_>) -> String {
    let ambiguous = if item.issuer.is_empty() {
        item.account.contains(':')
    } else {
        item.account
            .starts_with(&format!("{}:", item.issuer.trim()))
    };
    if ambiguous {
        format!("{}:{}", item.issuer, item.account)
    } else {
        item.account.to_owned()
    }
}

/// Groups encoded accounts greedily so each group's fields fit `budget` bytes.
fn pack(encoded: &[Zeroizing<Vec<u8>>], budget: usize) -> Vec<Vec<usize>> {
    let mut batches: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for (index, item) in encoded.iter().enumerate() {
        // Field key and length prefix of the account inside the payload.
        let size = item.len() + 1 + varint_len(item.len());
        if batches.is_empty() || used + size > budget {
            batches.push(Vec::new());
            used = 0;
        }
        if let Some(last) = batches.last_mut() {
            last.push(index);
        }
        used += size;
    }
    batches
}

fn varint_len(value: usize) -> usize {
    let bits = usize::BITS - value.leading_zeros();
    usize::try_from(bits.max(1).div_ceil(7)).unwrap_or(10)
}

fn random_batch_id() -> Result<u64, ExportError> {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).map_err(|_| ExportError::RandomUnavailable)?;
    // A positive int32, as Google Authenticator writes.
    Ok(u64::from(u32::from_be_bytes(bytes) >> 1))
}

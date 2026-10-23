//! The sealed file format shared by `vault.okv`, `local.okv` and backups (design/02-data.md §3.3).
//!
//! ```text
//! magic "OKV\0" | header length (u16, big endian) | header (CBOR map) | ciphertext with 16-byte tag
//! ```
//!
//! The header says how to find the key and carries the nonce. It is authenticated together with the
//! ciphertext (as associated data), so it cannot be changed without detection, yet it can be read without the
//! key, for example to learn that a master password is needed or that the file is from a newer version.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ciborium::Value;
use zeroize::Zeroizing;

use crate::codec::{MapReader, as_fixed, as_u64, extra_pairs, int};
use crate::model::Extra;

/// The first four bytes of every sealed file.
pub const MAGIC: &[u8; 4] = b"OKV\0";
/// The sealed format this version writes and opens.
pub const SEALED_FORMAT: u16 = 1;

const CIPHER_XCHACHA20_POLY1305: u64 = 1;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const LENGTH_LEN: usize = 2;
/// Headers are small; anything larger is not a header this program wrote.
const MAX_HEADER_LEN: usize = 4096;

const H_FORMAT: u64 = 1;
const H_KIND: u64 = 2;
const H_CIPHER: u64 = 3;
const H_KEY_SOURCE: u64 = 4;
const H_KEY_ID: u64 = 5;
const H_KDF: u64 = 6;
const H_NONCE: u64 = 7;
const KDF_SALT: u64 = 1;
const KDF_M_COST: u64 = 2;
const KDF_T_COST: u64 = 3;
const KDF_P_COST: u64 = 4;

/// What a sealed file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// `vault.okv`.
    Vault,
    /// `local.okv`: this device's history and sync records.
    Local,
    /// A backup made before a migration or a pairing.
    Backup,
}

/// Where the data key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The system keystore holds it.
    Keystore,
    /// It is derived from the master password with the header's KDF parameters.
    MasterPassword,
}

/// Argon2id parameters for a master-password file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Random salt.
    pub salt: [u8; 16],
    /// Memory in KiB.
    pub m_cost_kib: u32,
    /// Iterations.
    pub t_cost: u32,
    /// Parallelism.
    pub p_cost: u32,
}

/// The readable part of a sealed file. Unknown fields are kept and written back.
#[derive(Debug, Clone, PartialEq)]
pub struct SealedHeader {
    /// Sealed format version.
    pub format: u16,
    /// What the file holds.
    pub kind: FileKind,
    /// Where the data key comes from.
    pub key_source: KeySource,
    /// Identifies the keystore entry of the data key.
    pub key_id: [u8; 16],
    /// Present for master-password files.
    pub kdf: Option<KdfParams>,
    /// Fields from newer versions.
    pub extra: Extra,
}

impl SealedHeader {
    /// A header in the current format without KDF parameters.
    #[must_use]
    pub fn new(kind: FileKind, key_source: KeySource, key_id: [u8; 16]) -> Self {
        Self {
            format: SEALED_FORMAT,
            kind,
            key_source,
            key_id,
            kdf: None,
            extra: Extra::new(),
        }
    }
}

/// The 256-bit key that encrypts this device's files. Wiped on drop; `Debug` never shows it.
#[derive(Clone)]
pub struct DataKey(Zeroizing<[u8; KEY_LEN]>);

impl DataKey {
    /// Wraps existing key bytes, for example from the keystore.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// A new random key from the operating system.
    ///
    /// # Errors
    ///
    /// Returns [`SealError::RandomUnavailable`] when the system random source fails.
    pub fn generate() -> Result<Self, SealError> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        getrandom::fill(&mut *bytes).map_err(|_| SealError::RandomUnavailable)?;
        Ok(Self(bytes))
    }

    /// The raw key. Callers must not log or persist it unprotected.
    #[must_use]
    pub fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(redacted)")
    }
}

/// Why a file could not be sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    /// The system random source failed.
    #[error("the system random number generator is unavailable")]
    RandomUnavailable,
    /// The header is too large to write.
    #[error("the file header is too large")]
    HeaderTooLarge,
    /// The data is too large to encrypt in one file.
    #[error("the data is too large to encrypt")]
    DataTooLarge,
}

/// Why a sealed file could not be read or opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// The file ends before the header does.
    #[error("the file is too short")]
    TooShort,
    /// Not an OTPKeep file.
    #[error("this is not an OTPKeep file")]
    BadMagic,
    /// The header cannot be read.
    #[error("the file header is damaged")]
    HeaderInvalid,
    /// Written by a newer version.
    #[error("the file was written by a newer version (format {0})")]
    UnsupportedFormat(u16),
    /// Encrypted with a cipher this version does not know.
    #[error("the file uses an unknown cipher")]
    UnsupportedCipher,
    /// Wrong key, or the file was changed or damaged.
    #[error("the file cannot be decrypted: wrong key, or the file is damaged")]
    AuthFailed,
}

/// Seals `plaintext` under `header` with a new random nonce.
///
/// # Errors
///
/// Returns [`SealError`] when no random nonce is available or the header is too large.
pub fn seal(header: &SealedHeader, key: &DataKey, plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| SealError::RandomUnavailable)?;
    let mut header_bytes = Vec::new();
    // Serializing an in-memory value into a Vec cannot fail.
    let _ = ciborium::ser::into_writer(&header_to_value(header, &nonce), &mut header_bytes);
    let header_len = u16::try_from(header_bytes.len())
        .ok()
        .filter(|len| usize::from(*len) <= MAX_HEADER_LEN)
        .ok_or(SealError::HeaderTooLarge)?;

    let mut out =
        Vec::with_capacity(MAGIC.len() + LENGTH_LEN + header_bytes.len() + plaintext.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&header_len.to_be_bytes());
    out.extend_from_slice(&header_bytes);
    let ciphertext = cipher(key)
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad: &out,
            },
        )
        .map_err(|_| SealError::DataTooLarge)?;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Reads the header without the key.
///
/// # Errors
///
/// Returns [`OpenError`] when the bytes are not a sealed file with a readable header.
pub fn read_header(bytes: &[u8]) -> Result<SealedHeader, OpenError> {
    split(bytes).map(|parts| parts.header)
}

/// Opens a sealed file.
///
/// # Errors
///
/// Returns [`OpenError`]; [`OpenError::AuthFailed`] covers both a wrong key and any change to the file.
pub fn open(bytes: &[u8], key: &DataKey) -> Result<(SealedHeader, Zeroizing<Vec<u8>>), OpenError> {
    let parts = split(bytes)?;
    if parts.header.format != SEALED_FORMAT {
        return Err(OpenError::UnsupportedFormat(parts.header.format));
    }
    if parts.cipher != CIPHER_XCHACHA20_POLY1305 {
        return Err(OpenError::UnsupportedCipher);
    }
    let (aad, ciphertext) = bytes.split_at(parts.body_start);
    let plaintext = cipher(key)
        .decrypt(
            &XNonce::from(parts.nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| OpenError::AuthFailed)?;
    Ok((parts.header, Zeroizing::new(plaintext)))
}

struct Parts {
    header: SealedHeader,
    cipher: u64,
    nonce: [u8; NONCE_LEN],
    body_start: usize,
}

fn split(bytes: &[u8]) -> Result<Parts, OpenError> {
    let prefix = MAGIC.len() + LENGTH_LEN;
    if bytes.len() < prefix {
        return Err(OpenError::TooShort);
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(OpenError::BadMagic);
    }
    let header_len = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    if header_len > MAX_HEADER_LEN {
        return Err(OpenError::HeaderInvalid);
    }
    let body_start = prefix + header_len;
    let header_bytes = bytes.get(prefix..body_start).ok_or(OpenError::TooShort)?;
    let mut value: Value =
        ciborium::de::from_reader(header_bytes).map_err(|_| OpenError::HeaderInvalid)?;
    let (header, cipher, nonce) = header_from_value(&mut value).ok_or(OpenError::HeaderInvalid)?;
    Ok(Parts {
        header,
        cipher,
        nonce,
        body_start,
    })
}

fn header_to_value(header: &SealedHeader, nonce: &[u8; NONCE_LEN]) -> Value {
    let mut pairs = vec![
        (int(H_FORMAT), int(u64::from(header.format))),
        (int(H_KIND), int(kind_code(header.kind))),
        (int(H_CIPHER), int(CIPHER_XCHACHA20_POLY1305)),
        (int(H_KEY_SOURCE), int(source_code(header.key_source))),
        (int(H_KEY_ID), Value::Bytes(header.key_id.to_vec())),
        (int(H_NONCE), Value::Bytes(nonce.to_vec())),
    ];
    if let Some(kdf) = header.kdf {
        pairs.push((
            int(H_KDF),
            Value::Map(vec![
                (int(KDF_SALT), Value::Bytes(kdf.salt.to_vec())),
                (int(KDF_M_COST), int(u64::from(kdf.m_cost_kib))),
                (int(KDF_T_COST), int(u64::from(kdf.t_cost))),
                (int(KDF_P_COST), int(u64::from(kdf.p_cost))),
            ]),
        ));
    }
    pairs.extend(extra_pairs(&header.extra));
    Value::Map(pairs)
}

fn header_from_value(value: &mut Value) -> Option<(SealedHeader, u64, [u8; NONCE_LEN])> {
    const NAME: &str = "header";
    let mut map = MapReader::take(value, NAME).ok()?;
    let format = u16::try_from(as_u64(map.required(H_FORMAT, NAME).ok()?, NAME).ok()?).ok()?;
    let kind = kind_from_code(as_u64(map.required(H_KIND, NAME).ok()?, NAME).ok()?)?;
    let cipher = as_u64(map.required(H_CIPHER, NAME).ok()?, NAME).ok()?;
    let key_source = source_from_code(as_u64(map.required(H_KEY_SOURCE, NAME).ok()?, NAME).ok()?)?;
    let key_id = as_fixed(map.required(H_KEY_ID, NAME).ok()?, NAME).ok()?;
    let nonce = as_fixed(map.required(H_NONCE, NAME).ok()?, NAME).ok()?;
    let kdf = match map.optional(H_KDF) {
        Some(mut kdf) => Some(kdf_from_value(&mut kdf)?),
        None => None,
    };
    let header = SealedHeader {
        format,
        kind,
        key_source,
        key_id,
        kdf,
        extra: map.into_extra(),
    };
    Some((header, cipher, nonce))
}

fn kdf_from_value(value: &mut Value) -> Option<KdfParams> {
    const NAME: &str = "kdf";
    let mut map = MapReader::take(value, NAME).ok()?;
    let mut number = |key| -> Option<u32> {
        u32::try_from(as_u64(map.required(key, NAME).ok()?, NAME).ok()?).ok()
    };
    let (m_cost_kib, t_cost, p_cost) = (
        number(KDF_M_COST)?,
        number(KDF_T_COST)?,
        number(KDF_P_COST)?,
    );
    let salt = as_fixed(map.required(KDF_SALT, NAME).ok()?, NAME).ok()?;
    map.finish(NAME).ok()?;
    Some(KdfParams {
        salt,
        m_cost_kib,
        t_cost,
        p_cost,
    })
}

fn cipher(key: &DataKey) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&(*key.expose()).into())
}

const fn kind_code(kind: FileKind) -> u64 {
    match kind {
        FileKind::Vault => 1,
        FileKind::Local => 2,
        FileKind::Backup => 3,
    }
}

const fn kind_from_code(code: u64) -> Option<FileKind> {
    match code {
        1 => Some(FileKind::Vault),
        2 => Some(FileKind::Local),
        3 => Some(FileKind::Backup),
        _ => None,
    }
}

const fn source_code(source: KeySource) -> u64 {
    match source {
        KeySource::Keystore => 1,
        KeySource::MasterPassword => 2,
    }
}

const fn source_from_code(code: u64) -> Option<KeySource> {
    match code {
        1 => Some(KeySource::Keystore),
        2 => Some(KeySource::MasterPassword),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

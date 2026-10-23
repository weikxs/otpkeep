//! CBOR encoding of the vault with integer keys (design/02-data.md §1, §3.3).
//!
//! Unknown keys at the vault and entry level are kept in `extra` and written back unchanged, so a newer
//! version's data survives a round trip through this one. Inside key material and code specifications unknown
//! keys are rejected instead: dropping part of a secret's definition would silently change its codes, so any
//! change there needs a new format version.

use ciborium::Value;
use zeroize::{Zeroize, Zeroizing};

use crate::model::{DeviceTag, Extra, VaultDoc};

mod entry;
mod group;

/// The document format this version reads and writes.
pub const DOC_FORMAT: u64 = 1;

const KEY_FORMAT: u64 = 0;
const KEY_VAULT_ID: u64 = 1;
const KEY_ENTRIES: u64 = 2;
const KEY_TOMBSTONES: u64 = 3;
const KEY_CLOCK: u64 = 4;
const KEY_DEVICE_TAG: u64 = 5;
const KEY_GROUP: u64 = 6;
const KEY_DEVICE: u64 = 7;

/// Why stored bytes are not a vault this version can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Not CBOR at all.
    #[error("the data is not CBOR")]
    NotCbor,
    /// A required field is absent.
    #[error("the field {0} is missing")]
    Missing(&'static str),
    /// A field has the wrong type or an impossible value.
    #[error("the field {0} is invalid")]
    Invalid(&'static str),
    /// Written by a version with a different document format.
    #[error("the document format {0} is not supported")]
    UnsupportedFormat(u64),
}

/// Encodes the vault. The bytes contain secrets and are wiped from memory when dropped.
#[must_use]
pub fn encode_vault(doc: &VaultDoc) -> Zeroizing<Vec<u8>> {
    let mut pairs = vec![
        (int(KEY_FORMAT), int(DOC_FORMAT)),
        (int(KEY_VAULT_ID), Value::Bytes(doc.vault_id.to_vec())),
        (
            int(KEY_ENTRIES),
            Value::Array(doc.entries.iter().map(entry::entry_to_value).collect()),
        ),
        (
            int(KEY_TOMBSTONES),
            Value::Array(
                doc.tombstones
                    .iter()
                    .map(entry::tombstone_to_value)
                    .collect(),
            ),
        ),
        (int(KEY_CLOCK), entry::hlc_to_value(doc.clock)),
        (int(KEY_DEVICE_TAG), Value::Bytes(doc.device_tag.0.to_vec())),
    ];
    if let Some(group) = &doc.group {
        pairs.push((int(KEY_GROUP), group::group_to_value(group)));
    }
    if let Some(device) = &doc.device {
        pairs.push((int(KEY_DEVICE), group::secrets_to_value(device)));
    }
    pairs.extend(extra_pairs(&doc.extra));
    let mut value = Value::Map(pairs);
    let mut bytes = Zeroizing::new(Vec::new());
    // Serializing an in-memory value into a Vec cannot fail.
    let _ = ciborium::ser::into_writer(&value, &mut *bytes);
    wipe(&mut value);
    bytes
}

/// Decodes a vault written by [`encode_vault`].
///
/// # Errors
///
/// Returns [`DecodeError`] naming the first field that is missing or invalid.
pub fn decode_vault(bytes: &[u8]) -> Result<VaultDoc, DecodeError> {
    let mut value: Value = ciborium::de::from_reader(bytes).map_err(|_| DecodeError::NotCbor)?;
    let result = decode_doc(&mut value);
    wipe(&mut value);
    result
}

fn decode_doc(value: &mut Value) -> Result<VaultDoc, DecodeError> {
    let mut map = MapReader::take(value, "vault")?;
    let format = as_u64(map.required(KEY_FORMAT, "vault.format")?, "vault.format")?;
    if format != DOC_FORMAT {
        return Err(DecodeError::UnsupportedFormat(format));
    }
    let vault_id = as_array16(map.required(KEY_VAULT_ID, "vault.id")?, "vault.id")?;
    let entries = as_array(map.required(KEY_ENTRIES, "vault.entries")?, "vault.entries")?
        .iter_mut()
        .map(entry::entry_from_value)
        .collect::<Result<_, _>>()?;
    let tombstones = as_array(
        map.required(KEY_TOMBSTONES, "vault.tombstones")?,
        "vault.tombstones",
    )?
    .iter_mut()
    .map(entry::tombstone_from_value)
    .collect::<Result<_, _>>()?;
    let clock = entry::hlc_from_value(&map.required(KEY_CLOCK, "vault.clock")?)?;
    let device_tag = DeviceTag(as_fixed(
        map.required(KEY_DEVICE_TAG, "vault.device_tag")?,
        "vault.device_tag",
    )?);
    let group = map
        .optional(KEY_GROUP)
        .map(group::group_from_value)
        .transpose()?;
    let device = map
        .optional(KEY_DEVICE)
        .map(group::secrets_from_value)
        .transpose()?;
    Ok(VaultDoc {
        vault_id,
        entries,
        tombstones,
        group,
        device,
        clock,
        device_tag,
        extra: map.into_extra(),
    })
}

/// Reads a CBOR map with integer keys, taking fields out as they are used.
pub(crate) struct MapReader {
    pairs: Vec<(u64, Value)>,
}

impl MapReader {
    /// Takes the map out of `value`, leaving `Null` behind.
    pub(crate) fn take(value: &mut Value, name: &'static str) -> Result<Self, DecodeError> {
        let Value::Map(raw) = std::mem::replace(value, Value::Null) else {
            return Err(DecodeError::Invalid(name));
        };
        let mut pairs = Vec::with_capacity(raw.len());
        for (key, field) in raw {
            let key = as_u64(key, name)?;
            if pairs.iter().any(|(seen, _)| *seen == key) {
                return Err(DecodeError::Invalid(name));
            }
            pairs.push((key, field));
        }
        Ok(Self { pairs })
    }

    pub(crate) fn optional(&mut self, key: u64) -> Option<Value> {
        let index = self.pairs.iter().position(|(k, _)| *k == key)?;
        Some(self.pairs.swap_remove(index).1)
    }

    pub(crate) fn required(&mut self, key: u64, name: &'static str) -> Result<Value, DecodeError> {
        self.optional(key).ok_or(DecodeError::Missing(name))
    }

    /// What is left: kept as unknown fields.
    pub(crate) fn into_extra(self) -> Extra {
        self.pairs.into_iter().collect()
    }

    /// What is left must be nothing.
    pub(crate) fn finish(mut self, name: &'static str) -> Result<(), DecodeError> {
        if self.pairs.is_empty() {
            Ok(())
        } else {
            self.pairs.iter_mut().for_each(|(_, v)| wipe(v));
            Err(DecodeError::Invalid(name))
        }
    }
}

pub(crate) fn int(n: u64) -> Value {
    Value::Integer(n.into())
}

pub(crate) fn extra_pairs(extra: &Extra) -> impl Iterator<Item = (Value, Value)> + '_ {
    extra.iter().map(|(key, value)| (int(*key), value.clone()))
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "callers hand over fields they have taken out of a map"
)]
pub(crate) fn as_u64(value: Value, name: &'static str) -> Result<u64, DecodeError> {
    match value {
        Value::Integer(n) => u64::try_from(n).map_err(|_| DecodeError::Invalid(name)),
        _ => Err(DecodeError::Invalid(name)),
    }
}

fn as_array(value: Value, name: &'static str) -> Result<Vec<Value>, DecodeError> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err(DecodeError::Invalid(name)),
    }
}

fn as_array16(value: Value, name: &'static str) -> Result<[u8; 16], DecodeError> {
    as_fixed(value, name)
}

pub(crate) fn as_fixed<const N: usize>(
    value: Value,
    name: &'static str,
) -> Result<[u8; N], DecodeError> {
    match value {
        Value::Bytes(bytes) => bytes.try_into().map_err(|_| DecodeError::Invalid(name)),
        _ => Err(DecodeError::Invalid(name)),
    }
}

/// Overwrites every byte string and text in `value`, so secrets do not linger after (de)serialization.
pub(crate) fn wipe(value: &mut Value) {
    match value {
        Value::Bytes(bytes) => bytes.zeroize(),
        Value::Text(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Map(pairs) => pairs.iter_mut().for_each(|(k, v)| {
            wipe(k);
            wipe(v);
        }),
        Value::Tag(_, inner) => wipe(inner),
        _ => {}
    }
}

#[cfg(test)]
mod tests;

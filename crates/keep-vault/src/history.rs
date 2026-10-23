//! This device's edit history of names and notes, kept in `local.okv` and never synced
//! (PRD FR-3.11, design/02-data.md §4).

use ciborium::Value;
use zeroize::Zeroizing;

use crate::codec::{DecodeError, MapReader, as_fixed, as_u64, extra_pairs, int, wipe};
use crate::model::{EntryId, Extra};

/// Versions kept per entry and field.
pub const HISTORY_VERSIONS: usize = 5;
/// Edits closer together than this form one version.
pub const HISTORY_MERGE_WINDOW_MS: u64 = 10 * 60 * 1000;

const KEY_RECORDS: u64 = 1;
const REC_ENTRY: u64 = 1;
const REC_FIELD: u64 = 2;
const REC_VALUE: u64 = 3;
const REC_AT: u64 = 4;

/// Which field a version belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryField {
    /// The display name.
    Title,
    /// The note.
    Note,
}

/// One earlier value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryVersion {
    /// The entry.
    pub entry: EntryId,
    /// The field.
    pub field: HistoryField,
    /// The value before it was changed.
    pub value: String,
    /// When it was recorded, in Unix milliseconds.
    pub at_ms: u64,
}

/// Everything stored in `local.okv`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalDoc {
    /// Earlier values, oldest first.
    pub history: Vec<HistoryVersion>,
    /// Fields from newer versions.
    pub extra: Extra,
}

impl LocalDoc {
    /// Remembers `old_value` as an earlier version of `field`, unless the previous version is less than
    /// [`HISTORY_MERGE_WINDOW_MS`] old or holds the same value. Keeps [`HISTORY_VERSIONS`] per field.
    #[must_use]
    pub fn record(
        &self,
        entry: EntryId,
        field: HistoryField,
        old_value: &str,
        now_ms: u64,
    ) -> Self {
        if let Some(latest) = self.versions(entry, field).first()
            && (now_ms.saturating_sub(latest.at_ms) < HISTORY_MERGE_WINDOW_MS
                || latest.value == old_value)
        {
            return self.clone();
        }
        let mut history = self.history.clone();
        history.push(HistoryVersion {
            entry,
            field,
            value: old_value.to_owned(),
            at_ms: now_ms,
        });
        let count = history
            .iter()
            .filter(|v| v.entry == entry && v.field == field)
            .count();
        if count > HISTORY_VERSIONS
            && let Some(oldest) = history
                .iter()
                .position(|v| v.entry == entry && v.field == field)
        {
            history.remove(oldest);
        }
        Self {
            history,
            extra: self.extra.clone(),
        }
    }

    /// Earlier versions of one field, newest first.
    #[must_use]
    pub fn versions(&self, entry: EntryId, field: HistoryField) -> Vec<&HistoryVersion> {
        self.history
            .iter()
            .rev()
            .filter(|v| v.entry == entry && v.field == field)
            .collect()
    }

    /// Drops all history of an entry, for when it is deleted permanently.
    #[must_use]
    pub fn forget(&self, entry: EntryId) -> Self {
        let history = self
            .history
            .iter()
            .filter(|v| v.entry != entry)
            .cloned()
            .collect();
        Self {
            history,
            extra: self.extra.clone(),
        }
    }
}

/// Encodes `local.okv` contents. Notes are private, so the bytes are wiped when dropped.
#[must_use]
pub fn encode_local(local: &LocalDoc) -> Zeroizing<Vec<u8>> {
    let records = local
        .history
        .iter()
        .map(|v| {
            Value::Map(vec![
                (int(REC_ENTRY), Value::Bytes(v.entry.0.to_vec())),
                (int(REC_FIELD), int(field_code(v.field))),
                (int(REC_VALUE), Value::Text(v.value.clone())),
                (int(REC_AT), int(v.at_ms)),
            ])
        })
        .collect();
    let mut pairs = vec![(int(KEY_RECORDS), Value::Array(records))];
    pairs.extend(extra_pairs(&local.extra));
    let mut value = Value::Map(pairs);
    let mut bytes = Zeroizing::new(Vec::new());
    // Serializing an in-memory value into a Vec cannot fail.
    let _ = ciborium::ser::into_writer(&value, &mut *bytes);
    wipe(&mut value);
    bytes
}

/// Decodes `local.okv` contents.
///
/// # Errors
///
/// Returns [`DecodeError`] for damaged data.
pub fn decode_local(bytes: &[u8]) -> Result<LocalDoc, DecodeError> {
    let mut value: Value = ciborium::de::from_reader(bytes).map_err(|_| DecodeError::NotCbor)?;
    let result = decode(&mut value);
    wipe(&mut value);
    result
}

fn decode(value: &mut Value) -> Result<LocalDoc, DecodeError> {
    const NAME: &str = "local";
    let mut map = MapReader::take(value, NAME)?;
    let Value::Array(mut records) = map.required(KEY_RECORDS, "local.history")? else {
        return Err(DecodeError::Invalid("local.history"));
    };
    let history = records
        .iter_mut()
        .map(decode_version)
        .collect::<Result<_, _>>()?;
    Ok(LocalDoc {
        history,
        extra: map.into_extra(),
    })
}

fn decode_version(value: &mut Value) -> Result<HistoryVersion, DecodeError> {
    const NAME: &str = "local.history";
    let mut map = MapReader::take(value, NAME)?;
    let entry = EntryId(as_fixed(map.required(REC_ENTRY, NAME)?, NAME)?);
    let field = match as_u64(map.required(REC_FIELD, NAME)?, NAME)? {
        0 => HistoryField::Title,
        1 => HistoryField::Note,
        _ => return Err(DecodeError::Invalid(NAME)),
    };
    let Value::Text(value) = map.required(REC_VALUE, NAME)? else {
        return Err(DecodeError::Invalid(NAME));
    };
    let at_ms = as_u64(map.required(REC_AT, NAME)?, NAME)?;
    map.finish(NAME)?;
    Ok(HistoryVersion {
        entry,
        field,
        value,
        at_ms,
    })
}

const fn field_code(field: HistoryField) -> u64 {
    match field {
        HistoryField::Title => 0,
        HistoryField::Note => 1,
    }
}

#[cfg(test)]
mod tests;

//! Encoding of entries, key material, timestamps and tombstones.

use ciborium::Value;
use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};

use super::{DecodeError, MapReader, as_array, as_fixed, as_u64, extra_pairs, int};
use crate::model::{DeviceTag, Entry, EntryId, Field, Hlc, KeyMaterial, Tombstone};

const ENTRY_ID: u64 = 1;
const ENTRY_CREATED_AT: u64 = 2;
const ENTRY_TITLE: u64 = 3;
const ENTRY_ISSUER: u64 = 4;
const ENTRY_ACCOUNT: u64 = 5;
const ENTRY_KEY: u64 = 6;
const ENTRY_NOTE: u64 = 7;
const ENTRY_TAGS: u64 = 8;
const ENTRY_RANK: u64 = 9;
const ENTRY_PINNED: u64 = 10;
const ENTRY_KEEP_DUPLICATE: u64 = 11;
const ENTRY_TRASHED_AT: u64 = 12;
const ENTRY_OLD_KEY_OF: u64 = 13;

const KEY_SECRET: u64 = 1;
const KEY_SPEC: u64 = 2;
const KEY_COUNTER: u64 = 3;
const KEY_KEY_ID: u64 = 4;

const SPEC_KIND: u64 = 1;
const SPEC_PERIOD: u64 = 2;
const SPEC_DIGEST: u64 = 3;
const SPEC_DIGITS: u64 = 4;
const KIND_TOTP: u64 = 0;
const KIND_HOTP: u64 = 1;

const TOMB_ENTRY: u64 = 1;
const TOMB_STAMP: u64 = 2;
const TOMB_ACKED_BY: u64 = 3;

pub(super) fn entry_to_value(entry: &Entry) -> Value {
    let mut pairs = vec![
        (int(ENTRY_ID), Value::Bytes(entry.id.0.to_vec())),
        (int(ENTRY_CREATED_AT), hlc_to_value(entry.created_at)),
        (int(ENTRY_TITLE), field(&entry.title, |s| text(s))),
        (int(ENTRY_ISSUER), field(&entry.issuer, |s| text(s))),
        (int(ENTRY_ACCOUNT), field(&entry.account, |s| text(s))),
        (int(ENTRY_KEY), field(&entry.key, key_to_value)),
        (int(ENTRY_NOTE), field(&entry.note, |s| text(s))),
        (
            int(ENTRY_TAGS),
            field(&entry.tags, |tags| {
                Value::Array(tags.iter().map(|t| text(t)).collect())
            }),
        ),
        (int(ENTRY_RANK), field(&entry.rank, |s| text(s))),
        (int(ENTRY_PINNED), field(&entry.pinned, |b| Value::Bool(*b))),
        (
            int(ENTRY_KEEP_DUPLICATE),
            field(&entry.keep_duplicate, |b| Value::Bool(*b)),
        ),
        (
            int(ENTRY_TRASHED_AT),
            field(&entry.trashed_at, |at| at.map_or(Value::Null, hlc_to_value)),
        ),
        (
            int(ENTRY_OLD_KEY_OF),
            field(&entry.old_key_of, |of| {
                of.map_or(Value::Null, |id| Value::Bytes(id.0.to_vec()))
            }),
        ),
    ];
    pairs.extend(extra_pairs(&entry.extra));
    Value::Map(pairs)
}

pub(super) fn entry_from_value(value: &mut Value) -> Result<Entry, DecodeError> {
    let mut map = MapReader::take(value, "entry")?;
    let mut get = |key, name| map.required(key, name);
    let id = EntryId(as_fixed(get(ENTRY_ID, "entry.id")?, "entry.id")?);
    let created_at = hlc_from_value(&get(ENTRY_CREATED_AT, "entry.created_at")?)?;
    let title = read_field(get(ENTRY_TITLE, "entry.title")?, "entry.title", as_text)?;
    let issuer = read_field(get(ENTRY_ISSUER, "entry.issuer")?, "entry.issuer", as_text)?;
    let account = read_field(
        get(ENTRY_ACCOUNT, "entry.account")?,
        "entry.account",
        as_text,
    )?;
    let key = read_field(get(ENTRY_KEY, "entry.key")?, "entry.key", key_from_value)?;
    let note = read_field(get(ENTRY_NOTE, "entry.note")?, "entry.note", as_text)?;
    let tags = read_field(get(ENTRY_TAGS, "entry.tags")?, "entry.tags", as_tags)?;
    let rank = read_field(get(ENTRY_RANK, "entry.rank")?, "entry.rank", as_text)?;
    let pinned = read_field(get(ENTRY_PINNED, "entry.pinned")?, "entry.pinned", as_bool)?;
    let keep_duplicate = read_field(
        get(ENTRY_KEEP_DUPLICATE, "entry.keep_duplicate")?,
        "entry.keep_duplicate",
        as_bool,
    )?;
    let trashed_at = read_field(
        get(ENTRY_TRASHED_AT, "entry.trashed_at")?,
        "entry.trashed_at",
        |v, _| {
            if v == Value::Null {
                Ok(None)
            } else {
                hlc_from_value(&v).map(Some)
            }
        },
    )?;
    let old_key_of = read_field(
        get(ENTRY_OLD_KEY_OF, "entry.old_key_of")?,
        "entry.old_key_of",
        |v, name| {
            if v == Value::Null {
                Ok(None)
            } else {
                as_fixed(v, name).map(|id| Some(EntryId(id)))
            }
        },
    )?;
    Ok(Entry {
        id,
        created_at,
        title,
        issuer,
        account,
        key,
        note,
        tags,
        rank,
        pinned,
        keep_duplicate,
        trashed_at,
        old_key_of,
        extra: map.into_extra(),
    })
}

fn key_to_value(key: &KeyMaterial) -> Value {
    let spec = key.spec;
    let mut spec_pairs = Vec::with_capacity(4);
    match spec.kind() {
        CodeKind::TimeBased { period_s } => {
            spec_pairs.push((int(SPEC_KIND), int(KIND_TOTP)));
            spec_pairs.push((int(SPEC_PERIOD), int(u64::from(period_s))));
        }
        CodeKind::CounterBased => spec_pairs.push((int(SPEC_KIND), int(KIND_HOTP))),
    }
    spec_pairs.push((int(SPEC_DIGEST), int(digest_code(spec.digest()))));
    spec_pairs.push((int(SPEC_DIGITS), int(u64::from(spec.digits()))));
    Value::Map(vec![
        (int(KEY_SECRET), Value::Bytes(key.secret.expose().to_vec())),
        (int(KEY_SPEC), Value::Map(spec_pairs)),
        (int(KEY_COUNTER), int(key.counter)),
        (int(KEY_KEY_ID), hlc_to_value(key.key_id)),
    ])
}

fn key_from_value(mut value: Value, name: &'static str) -> Result<KeyMaterial, DecodeError> {
    let mut map = MapReader::take(&mut value, name)?;
    let secret = match map.required(KEY_SECRET, "key.secret")? {
        Value::Bytes(bytes) => {
            SecretKey::from_bytes(bytes).map_err(|_| DecodeError::Invalid("key.secret"))?
        }
        _ => return Err(DecodeError::Invalid("key.secret")),
    };
    let spec = spec_from_value(map.required(KEY_SPEC, "key.spec")?)?;
    let counter = as_u64(map.required(KEY_COUNTER, "key.counter")?, "key.counter")?;
    let key_id = hlc_from_value(&map.required(KEY_KEY_ID, "key.key_id")?)?;
    map.finish("key")?;
    Ok(KeyMaterial {
        secret,
        spec,
        counter,
        key_id,
    })
}

fn spec_from_value(mut value: Value) -> Result<CodeSpec, DecodeError> {
    const NAME: &str = "key.spec";
    let mut map = MapReader::take(&mut value, NAME)?;
    let kind = match as_u64(map.required(SPEC_KIND, NAME)?, NAME)? {
        KIND_TOTP => {
            let period = as_u64(map.required(SPEC_PERIOD, NAME)?, NAME)?;
            CodeKind::TimeBased {
                period_s: u16::try_from(period).map_err(|_| DecodeError::Invalid(NAME))?,
            }
        }
        KIND_HOTP => CodeKind::CounterBased,
        _ => return Err(DecodeError::Invalid(NAME)),
    };
    let digest = digest_from_code(as_u64(map.required(SPEC_DIGEST, NAME)?, NAME)?)?;
    let digits = u8::try_from(as_u64(map.required(SPEC_DIGITS, NAME)?, NAME)?)
        .map_err(|_| DecodeError::Invalid(NAME))?;
    map.finish(NAME)?;
    CodeSpec::new(kind, digest, digits).map_err(|_| DecodeError::Invalid(NAME))
}

const fn digest_code(digest: Digest) -> u64 {
    match digest {
        Digest::Sha1 => 0,
        Digest::Sha256 => 1,
        Digest::Sha512 => 2,
        Digest::Md5 => 3,
    }
}

fn digest_from_code(code: u64) -> Result<Digest, DecodeError> {
    match code {
        0 => Ok(Digest::Sha1),
        1 => Ok(Digest::Sha256),
        2 => Ok(Digest::Sha512),
        3 => Ok(Digest::Md5),
        _ => Err(DecodeError::Invalid("key.spec")),
    }
}

pub(super) fn tombstone_to_value(tomb: &Tombstone) -> Value {
    Value::Map(vec![
        (int(TOMB_ENTRY), Value::Bytes(tomb.entry.0.to_vec())),
        (int(TOMB_STAMP), hlc_to_value(tomb.stamp)),
        (
            int(TOMB_ACKED_BY),
            Value::Array(
                tomb.acked_by
                    .iter()
                    .map(|d| Value::Bytes(d.to_vec()))
                    .collect(),
            ),
        ),
    ])
}

pub(super) fn tombstone_from_value(value: &mut Value) -> Result<Tombstone, DecodeError> {
    let mut map = MapReader::take(value, "tombstone")?;
    let entry = EntryId(as_fixed(
        map.required(TOMB_ENTRY, "tombstone.entry")?,
        "tombstone.entry",
    )?);
    let stamp = hlc_from_value(&map.required(TOMB_STAMP, "tombstone.stamp")?)?;
    let acked_by = as_array(
        map.required(TOMB_ACKED_BY, "tombstone.acked_by")?,
        "tombstone.acked_by",
    )?
    .into_iter()
    .map(|device| as_fixed(device, "tombstone.acked_by"))
    .collect::<Result<_, _>>()?;
    map.finish("tombstone")?;
    Ok(Tombstone {
        entry,
        stamp,
        acked_by,
    })
}

pub(super) fn hlc_to_value(hlc: Hlc) -> Value {
    Value::Array(vec![
        int(hlc.wall_ms),
        int(u64::from(hlc.logical)),
        Value::Bytes(hlc.device.0.to_vec()),
    ])
}

pub(super) fn hlc_from_value(value: &Value) -> Result<Hlc, DecodeError> {
    const NAME: &str = "timestamp";
    let Value::Array(parts) = value else {
        return Err(DecodeError::Invalid(NAME));
    };
    let [wall_ms, logical, device] = parts.as_slice() else {
        return Err(DecodeError::Invalid(NAME));
    };
    Ok(Hlc {
        wall_ms: as_u64(wall_ms.clone(), NAME)?,
        logical: u32::try_from(as_u64(logical.clone(), NAME)?)
            .map_err(|_| DecodeError::Invalid(NAME))?,
        device: DeviceTag(as_fixed(device.clone(), NAME)?),
    })
}

fn field<T>(field: &Field<T>, encode: impl Fn(&T) -> Value) -> Value {
    Value::Array(vec![encode(&field.value), hlc_to_value(field.stamp)])
}

fn read_field<T>(
    value: Value,
    name: &'static str,
    decode: impl Fn(Value, &'static str) -> Result<T, DecodeError>,
) -> Result<Field<T>, DecodeError> {
    let Value::Array(parts) = value else {
        return Err(DecodeError::Invalid(name));
    };
    let Ok::<[Value; 2], _>([inner, stamp]) = parts.try_into() else {
        return Err(DecodeError::Invalid(name));
    };
    Ok(Field {
        value: decode(inner, name)?,
        stamp: hlc_from_value(&stamp)?,
    })
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

fn as_text(value: Value, name: &'static str) -> Result<String, DecodeError> {
    match value {
        Value::Text(text) => Ok(text),
        _ => Err(DecodeError::Invalid(name)),
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "shares the signature of the other field decoders"
)]
fn as_bool(value: Value, name: &'static str) -> Result<bool, DecodeError> {
    match value {
        Value::Bool(b) => Ok(b),
        _ => Err(DecodeError::Invalid(name)),
    }
}

fn as_tags(value: Value, name: &'static str) -> Result<Vec<String>, DecodeError> {
    as_array(value, name)?
        .into_iter()
        .map(|tag| as_text(tag, name))
        .collect()
}

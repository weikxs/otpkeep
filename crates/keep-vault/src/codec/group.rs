//! CBOR encoding of the sync group and this device's keys.
//!
//! A membership event's kind is kept exactly as received when this version does not know it, or knows it but
//! finds extra fields: it is signed, so it must travel on unchanged, and it is then ignored for membership.

use ciborium::Value;
use zeroize::Zeroizing;

use super::entry::{hlc_from_value, hlc_to_value};
use super::{DecodeError, MapReader, as_fixed, as_u64, extra_pairs, int};
use crate::model::{DeviceId, DeviceSecrets, Hlc, MemberEvent, MemberKind, Platform, SyncGroup};

const GROUP_ID: u64 = 1;
const GROUP_TAG_KEY: u64 = 2;
const GROUP_EVENTS: u64 = 3;
const GROUP_PRESENCE: u64 = 4;

const EVENT_KIND: u64 = 1;
const EVENT_AUTHOR: u64 = 2;
const EVENT_STAMP: u64 = 3;
const EVENT_SIGNATURE: u64 = 4;

const KIND_CODE: u64 = 0;
const KIND_DEVICE: u64 = 1;
const JOIN_DX_PUBLIC: u64 = 2;
const JOIN_DX_CERT: u64 = 3;
const JOIN_PLATFORM: u64 = 4;
const RENAME_LAN_NAME: u64 = 2;

const KIND_JOIN: u64 = 1;
const KIND_REMOVE: u64 = 2;
const KIND_RENAME: u64 = 3;

const SECRETS_SIGNING_SEED: u64 = 1;
const SECRETS_EXCHANGE_SECRET: u64 = 2;

pub(super) fn group_to_value(group: &SyncGroup) -> Value {
    let mut pairs = vec![
        (int(GROUP_ID), Value::Bytes(group.group_id.to_vec())),
        (int(GROUP_TAG_KEY), Value::Bytes(group.tag_key.to_vec())),
        (
            int(GROUP_EVENTS),
            Value::Array(group.events.iter().map(event_to_value).collect()),
        ),
        (
            int(GROUP_PRESENCE),
            Value::Array(
                group
                    .presence
                    .iter()
                    .map(|(device, seen)| {
                        Value::Array(vec![Value::Bytes(device.0.to_vec()), hlc_to_value(*seen)])
                    })
                    .collect(),
            ),
        ),
    ];
    pairs.extend(extra_pairs(&group.extra));
    Value::Map(pairs)
}

pub(super) fn group_from_value(mut value: Value) -> Result<SyncGroup, DecodeError> {
    let mut map = MapReader::take(&mut value, "group")?;
    let group_id = as_fixed(map.required(GROUP_ID, "group.id")?, "group.id")?;
    let tag_key = as_fixed(
        map.required(GROUP_TAG_KEY, "group.tag_key")?,
        "group.tag_key",
    )?;
    let events = list(map.required(GROUP_EVENTS, "group.events")?, "group.events")?
        .into_iter()
        .map(event_from_value)
        .collect::<Result<_, _>>()?;
    let presence = list(
        map.required(GROUP_PRESENCE, "group.presence")?,
        "group.presence",
    )?
    .into_iter()
    .map(presence_from_value)
    .collect::<Result<_, _>>()?;
    Ok(SyncGroup {
        group_id,
        tag_key,
        events,
        presence,
        extra: map.into_extra(),
    })
}

fn presence_from_value(value: Value) -> Result<(DeviceId, Hlc), DecodeError> {
    const NAME: &str = "group.presence";
    let Value::Array(parts) = value else {
        return Err(DecodeError::Invalid(NAME));
    };
    let [device, seen] = <[Value; 2]>::try_from(parts).map_err(|_| DecodeError::Invalid(NAME))?;
    Ok((DeviceId(as_fixed(device, NAME)?), hlc_from_value(&seen)?))
}

/// The CBOR of one event, as stored and as sent.
pub(super) fn event_to_value(event: &MemberEvent) -> Value {
    Value::Map(vec![
        (int(EVENT_KIND), kind_to_value(&event.kind)),
        (int(EVENT_AUTHOR), Value::Bytes(event.author.0.to_vec())),
        (int(EVENT_STAMP), hlc_to_value(event.stamp)),
        (int(EVENT_SIGNATURE), Value::Bytes(event.signature.to_vec())),
    ])
}

fn event_from_value(mut value: Value) -> Result<MemberEvent, DecodeError> {
    let mut map = MapReader::take(&mut value, "event")?;
    let kind = kind_from_value(map.required(EVENT_KIND, "event.kind")?)?;
    let author = DeviceId(as_fixed(
        map.required(EVENT_AUTHOR, "event.author")?,
        "event.author",
    )?);
    let stamp = hlc_from_value(&map.required(EVENT_STAMP, "event.stamp")?)?;
    let signature = as_fixed(
        map.required(EVENT_SIGNATURE, "event.signature")?,
        "event.signature",
    )?;
    map.finish("event")?;
    Ok(MemberEvent {
        kind,
        author,
        stamp,
        signature,
    })
}

/// The CBOR of an event's kind: the part of the event, besides author and time, that is signed.
pub(crate) fn kind_to_value(kind: &MemberKind) -> Value {
    let device = |d: &DeviceId| (int(KIND_DEVICE), Value::Bytes(d.0.to_vec()));
    match kind {
        MemberKind::Join {
            device: joined,
            dx_public,
            dx_cert,
            platform,
        } => Value::Map(vec![
            (int(KIND_CODE), int(KIND_JOIN)),
            device(joined),
            (int(JOIN_DX_PUBLIC), Value::Bytes(dx_public.to_vec())),
            (int(JOIN_DX_CERT), Value::Bytes(dx_cert.to_vec())),
            (int(JOIN_PLATFORM), int(platform_code(*platform))),
        ]),
        MemberKind::Remove { device: removed } => {
            Value::Map(vec![(int(KIND_CODE), int(KIND_REMOVE)), device(removed)])
        }
        MemberKind::Rename {
            device: renamed,
            lan_name,
        } => Value::Map(vec![
            (int(KIND_CODE), int(KIND_RENAME)),
            device(renamed),
            (int(RENAME_LAN_NAME), Value::Text(lan_name.clone())),
        ]),
        MemberKind::Other { body, .. } => body.clone(),
    }
}

fn kind_from_value(value: Value) -> Result<MemberKind, DecodeError> {
    let code = kind_code(&value)?;
    match known_kind(code, value.clone()) {
        Some(kind) => Ok(kind),
        None => Ok(MemberKind::Other {
            kind: code,
            body: value,
        }),
    }
}

fn kind_code(value: &Value) -> Result<u64, DecodeError> {
    const NAME: &str = "event.kind";
    let Value::Map(pairs) = value else {
        return Err(DecodeError::Invalid(NAME));
    };
    let code = pairs
        .iter()
        .find(|(k, _)| *k == int(KIND_CODE))
        .map(|(_, v)| v.clone())
        .ok_or(DecodeError::Missing(NAME))?;
    as_u64(code, NAME)
}

/// A kind this version understands completely, or `None` to keep it as received.
fn known_kind(code: u64, mut value: Value) -> Option<MemberKind> {
    let mut map = MapReader::take(&mut value, "event.kind").ok()?;
    map.optional(KIND_CODE)?;
    let device = DeviceId(as_fixed(map.optional(KIND_DEVICE)?, "event.kind").ok()?);
    let kind = match code {
        KIND_JOIN => MemberKind::Join {
            device,
            dx_public: as_fixed(map.optional(JOIN_DX_PUBLIC)?, "event.kind").ok()?,
            dx_cert: as_fixed(map.optional(JOIN_DX_CERT)?, "event.kind").ok()?,
            platform: platform_from_code(as_u64(map.optional(JOIN_PLATFORM)?, "event.kind").ok()?),
        },
        KIND_REMOVE => MemberKind::Remove { device },
        KIND_RENAME => match map.optional(RENAME_LAN_NAME)? {
            Value::Text(lan_name) => MemberKind::Rename { device, lan_name },
            _ => return None,
        },
        _ => return None,
    };
    map.finish("event.kind").ok()?;
    Some(kind)
}

const fn platform_code(platform: Platform) -> u64 {
    match platform {
        Platform::MacOs => 1,
        Platform::Windows => 2,
        Platform::Linux => 3,
        Platform::Ios => 4,
        Platform::Android => 5,
        Platform::Other(code) => code,
    }
}

const fn platform_from_code(code: u64) -> Platform {
    match code {
        1 => Platform::MacOs,
        2 => Platform::Windows,
        3 => Platform::Linux,
        4 => Platform::Ios,
        5 => Platform::Android,
        other => Platform::Other(other),
    }
}

pub(super) fn secrets_to_value(secrets: &DeviceSecrets) -> Value {
    Value::Map(vec![
        (
            int(SECRETS_SIGNING_SEED),
            Value::Bytes(secrets.signing_seed.to_vec()),
        ),
        (
            int(SECRETS_EXCHANGE_SECRET),
            Value::Bytes(secrets.exchange_secret.to_vec()),
        ),
    ])
}

pub(super) fn secrets_from_value(mut value: Value) -> Result<DeviceSecrets, DecodeError> {
    let mut map = MapReader::take(&mut value, "device")?;
    let signing_seed = secret_key(
        map.required(SECRETS_SIGNING_SEED, "device.signing_seed")?,
        "device.signing_seed",
    )?;
    let exchange_secret = secret_key(
        map.required(SECRETS_EXCHANGE_SECRET, "device.exchange_secret")?,
        "device.exchange_secret",
    )?;
    map.finish("device")?;
    Ok(DeviceSecrets {
        signing_seed,
        exchange_secret,
    })
}

/// A 32-byte private key; the decoded bytes are wiped whether or not they fit.
fn secret_key(value: Value, name: &'static str) -> Result<Zeroizing<[u8; 32]>, DecodeError> {
    let Value::Bytes(bytes) = value else {
        return Err(DecodeError::Invalid(name));
    };
    let bytes = Zeroizing::new(bytes);
    let mut key = Zeroizing::new([0u8; 32]);
    if bytes.len() != key.len() {
        return Err(DecodeError::Invalid(name));
    }
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn list(value: Value, name: &'static str) -> Result<Vec<Value>, DecodeError> {
    match value {
        Value::Array(items) => Ok(items),
        _ => Err(DecodeError::Invalid(name)),
    }
}

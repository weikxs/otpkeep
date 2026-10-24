//! The data-phase messages of a session, carried in [`crate::channel::message::Message::Data`].
//!
//! Partial states travel as vault documents (the vault's own encoding, so fields from newer versions survive):
//! membership events in one, entries and tombstones in batches.

use ciborium::Value;
use keep_vault::{
    DeviceTag, Entry, EntryId, Extra, Hlc, SyncGroup, Tombstone, VaultDoc, decode_vault,
    encode_vault,
};
use zeroize::Zeroizing;

const KEY_KIND: u64 = 0;
const KEY_BODY: u64 = 1;
const KIND_EVENTS: u64 = 1;
const KIND_SUMMARY: u64 = 2;
const KIND_ENTRIES: u64 = 3;
const KIND_DONE: u64 = 4;
const KIND_DIGEST: u64 = 5;
const KIND_CERTIFICATE: u64 = 6;
const SUMMARY_ITEM: usize = 16 + 32;

/// One data-phase message.
#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    /// The sender's group: membership events and presence.
    Events(VaultDoc),
    /// A hash of each of the sender's entries.
    Summary(Vec<(EntryId, [u8; 32])>),
    /// Entries the receiver lacks or holds differently, and tombstones.
    Entries(VaultDoc),
    /// No more entries in this step.
    Done,
    /// The sender's state digest.
    Digest([u8; 32]),
    /// Pairing: the joiner's certificate for its exchange key, made once it knows the group.
    Certificate([u8; 64]),
}

impl Data {
    /// The bytes to send.
    #[must_use]
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let int = |n: u64| Value::Integer(n.into());
        let (kind, body) = match self {
            Self::Events(doc) => (KIND_EVENTS, Some(Value::Bytes(encode_vault(doc).to_vec()))),
            Self::Summary(items) => {
                let mut flat = Vec::with_capacity(items.len() * SUMMARY_ITEM);
                for (id, hash) in items {
                    flat.extend_from_slice(&id.0);
                    flat.extend_from_slice(hash);
                }
                (KIND_SUMMARY, Some(Value::Bytes(flat)))
            }
            Self::Entries(doc) => (KIND_ENTRIES, Some(Value::Bytes(encode_vault(doc).to_vec()))),
            Self::Done => (KIND_DONE, None),
            Self::Digest(digest) => (KIND_DIGEST, Some(Value::Bytes(digest.to_vec()))),
            Self::Certificate(cert) => (KIND_CERTIFICATE, Some(Value::Bytes(cert.to_vec()))),
        };
        let mut pairs = vec![(int(KEY_KIND), int(kind))];
        pairs.extend(body.map(|b| (int(KEY_BODY), b)));
        let mut value = Value::Map(pairs);
        let mut bytes = Zeroizing::new(Vec::new());
        // Serializing an in-memory value into a Vec cannot fail.
        let _ = ciborium::ser::into_writer(&value, &mut *bytes);
        if let Value::Map(pairs) = &mut value {
            for (_, v) in pairs.iter_mut() {
                if let Value::Bytes(b) = v {
                    zeroize::Zeroize::zeroize(b);
                }
            }
        }
        bytes
    }

    /// Reads a message; `None` for anything malformed.
    #[must_use]
    pub fn decode(input: &[u8]) -> Option<Self> {
        let Value::Map(pairs) = ciborium::de::from_reader(input).ok()? else {
            return None;
        };
        let get = |key: u64| {
            pairs
                .iter()
                .find(|(k, _)| *k == Value::Integer(key.into()))
                .map(|(_, v)| v)
        };
        let Value::Integer(kind) = get(KEY_KIND)? else {
            return None;
        };
        let body = || match get(KEY_BODY) {
            Some(Value::Bytes(bytes)) => Some(Zeroizing::new(bytes.clone())),
            _ => None,
        };
        match u64::try_from(*kind).ok()? {
            KIND_EVENTS => Some(Self::Events(decode_vault(&body()?).ok()?)),
            KIND_SUMMARY => {
                let flat = body()?;
                if flat.len() % SUMMARY_ITEM != 0 {
                    return None;
                }
                let items = flat
                    .chunks(SUMMARY_ITEM)
                    .map(|item| {
                        let id: [u8; 16] = item[..16].try_into().ok()?;
                        let hash: [u8; 32] = item[16..].try_into().ok()?;
                        Some((EntryId(id), hash))
                    })
                    .collect::<Option<_>>()?;
                Some(Self::Summary(items))
            }
            KIND_ENTRIES => Some(Self::Entries(decode_vault(&body()?).ok()?)),
            KIND_DONE => Some(Self::Done),
            KIND_DIGEST => Some(Self::Digest(body()?.as_slice().try_into().ok()?)),
            KIND_CERTIFICATE => Some(Self::Certificate(body()?.as_slice().try_into().ok()?)),
            _ => None,
        }
    }
}

/// A document holding only what is sent: no vault id, device keys or local fields.
#[must_use]
pub fn partial(
    entries: Vec<Entry>,
    tombstones: Vec<Tombstone>,
    group: Option<SyncGroup>,
    clock: Hlc,
) -> VaultDoc {
    VaultDoc {
        vault_id: [0; 16],
        entries,
        tombstones,
        group,
        device: None,
        clock,
        device_tag: DeviceTag([0; 8]),
        extra: Extra::new(),
    }
}

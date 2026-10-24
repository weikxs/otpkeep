//! Messages inside the channel, as CBOR maps with the kind under key 0.

use ciborium::Value;
use zeroize::Zeroizing;

const KEY_KIND: u64 = 0;
const KEY_A: u64 = 1;
const KEY_B: u64 = 2;

const KIND_CODE_START: u64 = 1;
const KIND_CODE_REPLY: u64 = 2;
const KIND_CONFIRM: u64 = 3;
const KIND_ERROR: u64 = 4;
const KIND_KEEPALIVE: u64 = 5;
const KIND_AGREED: u64 = 6;
const KIND_DATA: u64 = 7;

/// A stop reason sent to the other side (design/10-reference.md §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Another connection is being handled.
    Busy,
    /// The code was wrong.
    CodeWrong,
    /// Too many wrong codes; the window closed.
    CodeLocked,
    /// The device is not in the sync group.
    NotInGroup,
    /// The device was removed.
    Removed,
    /// The joining device is already a member.
    AlreadyMember,
    /// The joining device is in another group.
    OtherGroup,
    /// The group has 16 devices.
    GroupFull,
    /// No common protocol version.
    VersionMismatch,
    /// A timestamp is far ahead.
    PeerClock,
    /// Too much data.
    TooLarge,
    /// The two sides computed different results.
    DigestMismatch,
    /// The person declined the sync request.
    Rejected,
    /// The check codes did not match.
    SasMismatch,
    /// Someone cancelled, or a timer ran out.
    Cancelled,
    /// The data is read-only.
    ReadOnly,
    /// The message was not understood.
    Protocol,
}

const ERROR_CODES: &[(ErrorCode, &str)] = &[
    (ErrorCode::Busy, "BUSY"),
    (ErrorCode::CodeWrong, "CODE_WRONG"),
    (ErrorCode::CodeLocked, "CODE_LOCKED"),
    (ErrorCode::NotInGroup, "NOT_IN_GROUP"),
    (ErrorCode::Removed, "REMOVED"),
    (ErrorCode::AlreadyMember, "ALREADY_MEMBER"),
    (ErrorCode::OtherGroup, "OTHER_GROUP"),
    (ErrorCode::GroupFull, "GROUP_FULL"),
    (ErrorCode::VersionMismatch, "VERSION_MISMATCH"),
    (ErrorCode::PeerClock, "PEER_CLOCK"),
    (ErrorCode::TooLarge, "TOO_LARGE"),
    (ErrorCode::DigestMismatch, "DIGEST_MISMATCH"),
    (ErrorCode::Rejected, "REJECTED"),
    (ErrorCode::SasMismatch, "SAS_MISMATCH"),
    (ErrorCode::Cancelled, "CANCELLED"),
    (ErrorCode::ReadOnly, "READ_ONLY"),
    (ErrorCode::Protocol, "PROTOCOL"),
];

impl ErrorCode {
    /// The stable code shown in logs and the sync record.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        ERROR_CODES
            .iter()
            .find(|(code, _)| *code == self)
            .map_or("PROTOCOL", |(_, text)| text)
    }

    fn parse(text: &str) -> Self {
        ERROR_CODES
            .iter()
            .find(|(_, t)| *t == text)
            .map_or(Self::Protocol, |(code, _)| *code)
    }
}

/// One message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Joiner → opener: which secret it uses and its first code-check message.
    CodeStart {
        /// The secret's kind.
        mode: u8,
        /// The SPAKE2 message.
        spake: Vec<u8>,
    },
    /// Opener → joiner: its code-check message.
    CodeReply {
        /// The SPAKE2 message.
        spake: Vec<u8>,
    },
    /// Key confirmation.
    Confirm {
        /// HMAC over the side's label.
        mac: [u8; 32],
    },
    /// The sender stops.
    Error(ErrorCode),
    /// Sent every ten seconds while a person decides.
    Keepalive,
    /// The person on this side agreed (check code matches, or sync allowed).
    Agreed,
    /// Session data, encoded by the session.
    Data(Zeroizing<Vec<u8>>),
}

impl Message {
    /// The bytes to send.
    #[must_use]
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let int = |n: u64| Value::Integer(n.into());
        let mut pairs = Vec::new();
        let kind = match self {
            Self::CodeStart { mode, spake } => {
                pairs.push((int(KEY_A), int(u64::from(*mode))));
                pairs.push((int(KEY_B), Value::Bytes(spake.clone())));
                KIND_CODE_START
            }
            Self::CodeReply { spake } => {
                pairs.push((int(KEY_A), Value::Bytes(spake.clone())));
                KIND_CODE_REPLY
            }
            Self::Confirm { mac } => {
                pairs.push((int(KEY_A), Value::Bytes(mac.to_vec())));
                KIND_CONFIRM
            }
            Self::Error(code) => {
                pairs.push((int(KEY_A), Value::Text(code.as_str().to_owned())));
                KIND_ERROR
            }
            Self::Keepalive => KIND_KEEPALIVE,
            Self::Agreed => KIND_AGREED,
            Self::Data(bytes) => {
                pairs.push((int(KEY_A), Value::Bytes(bytes.to_vec())));
                KIND_DATA
            }
        };
        pairs.insert(0, (int(KEY_KIND), int(kind)));
        let mut value = Value::Map(pairs);
        let mut bytes = Zeroizing::new(Vec::new());
        // Serializing an in-memory value into a Vec cannot fail.
        let _ = ciborium::ser::into_writer(&value, &mut *bytes);
        wipe(&mut value);
        bytes
    }

    /// Reads a message; `None` for anything malformed.
    #[must_use]
    pub fn decode(input: &[u8]) -> Option<Self> {
        let Value::Map(mut pairs) = ciborium::de::from_reader(input).ok()? else {
            return None;
        };
        let mut take = |key: u64| {
            let index = pairs
                .iter()
                .position(|(k, _)| *k == Value::Integer(key.into()))?;
            Some(pairs.swap_remove(index).1)
        };
        let kind = uint(&take(KEY_KIND)?)?;
        let message = match kind {
            KIND_CODE_START => Self::CodeStart {
                mode: u8::try_from(uint(&take(KEY_A)?)?).ok()?,
                spake: bytes(take(KEY_B)?)?,
            },
            KIND_CODE_REPLY => Self::CodeReply {
                spake: bytes(take(KEY_A)?)?,
            },
            KIND_CONFIRM => Self::Confirm {
                mac: bytes(take(KEY_A)?)?.try_into().ok()?,
            },
            KIND_ERROR => match take(KEY_A)? {
                Value::Text(text) => Self::Error(ErrorCode::parse(&text)),
                _ => return None,
            },
            KIND_KEEPALIVE => Self::Keepalive,
            KIND_AGREED => Self::Agreed,
            KIND_DATA => Self::Data(Zeroizing::new(bytes(take(KEY_A)?)?)),
            _ => return None,
        };
        Some(message)
    }
}

fn uint(value: &Value) -> Option<u64> {
    match value {
        Value::Integer(n) => u64::try_from(*n).ok(),
        _ => None,
    }
}

fn bytes(value: Value) -> Option<Vec<u8>> {
    match value {
        Value::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}

/// Overwrites byte strings in a value that held secrets.
fn wipe(value: &mut Value) {
    use zeroize::Zeroize;
    match value {
        Value::Bytes(bytes) => bytes.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Map(pairs) => pairs.iter_mut().for_each(|(k, v)| {
            wipe(k);
            wipe(v);
        }),
        _ => {}
    }
}

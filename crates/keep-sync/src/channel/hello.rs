//! What each side says about itself in the Noise handshake.

use ciborium::Value;
use keep_vault::{DeviceId, Platform};

const KEY_DEVICE: u64 = 1;
const KEY_DX_CERT: u64 = 2;
const KEY_PLATFORM: u64 = 3;
const KEY_LAN_NAME: u64 = 4;
const KEY_VERSIONS: u64 = 5;
/// The longest LAN name (design/06-sync.md §2).
pub const LAN_NAME_MAX_CHARS: usize = 32;
const MAX_VERSIONS: usize = 16;

/// A device's introduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// Its identity.
    pub device: DeviceId,
    /// The certificate binding its exchange key to it and the group; absent for a device joining a group,
    /// which has no group yet.
    pub dx_cert: Option<[u8; 64]>,
    /// Its platform.
    pub platform: Platform,
    /// Its LAN name.
    pub lan_name: String,
    /// The protocol versions it speaks.
    pub versions: Vec<u32>,
}

/// The introduction could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the introduction is not valid")]
pub struct HelloError;

impl Hello {
    /// The CBOR sent in the handshake.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let int = |n: u64| Value::Integer(n.into());
        let mut pairs = vec![
            (int(KEY_DEVICE), Value::Bytes(self.device.0.to_vec())),
            (int(KEY_PLATFORM), int(platform_code(self.platform))),
            (int(KEY_LAN_NAME), Value::Text(self.lan_name.clone())),
            (
                int(KEY_VERSIONS),
                Value::Array(self.versions.iter().map(|v| int(u64::from(*v))).collect()),
            ),
        ];
        if let Some(cert) = self.dx_cert {
            pairs.push((int(KEY_DX_CERT), Value::Bytes(cert.to_vec())));
        }
        let mut bytes = Vec::new();
        // Serializing an in-memory value into a Vec cannot fail.
        let _ = ciborium::ser::into_writer(&Value::Map(pairs), &mut bytes);
        bytes
    }

    /// Reads an introduction. Unknown fields are ignored; a newer version may add some.
    ///
    /// # Errors
    ///
    /// Returns [`HelloError`] for anything malformed, including a LAN name that breaks the rules.
    pub fn decode(bytes: &[u8]) -> Result<Self, HelloError> {
        let Value::Map(pairs) = ciborium::de::from_reader(bytes).map_err(|_| HelloError)? else {
            return Err(HelloError);
        };
        let field = |key: u64| {
            pairs
                .iter()
                .find(|(k, _)| *k == Value::Integer(key.into()))
                .map(|(_, v)| v)
        };
        let device = DeviceId(bytes_of(field(KEY_DEVICE).ok_or(HelloError)?)?);
        let dx_cert = field(KEY_DX_CERT).map(bytes_of).transpose()?;
        let platform = platform_from_code(uint(field(KEY_PLATFORM).ok_or(HelloError)?)?);
        let lan_name = match field(KEY_LAN_NAME) {
            Some(Value::Text(name)) if is_valid_lan_name(name) => name.clone(),
            _ => return Err(HelloError),
        };
        let Some(Value::Array(list)) = field(KEY_VERSIONS) else {
            return Err(HelloError);
        };
        if list.len() > MAX_VERSIONS {
            return Err(HelloError);
        }
        let versions = list
            .iter()
            .map(|v| uint(v).and_then(|n| u32::try_from(n).map_err(|_| HelloError)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            device,
            dx_cert,
            platform,
            lan_name,
            versions,
        })
    }
}

/// 1–32 characters, no surrounding spaces, no control characters (design/06-sync.md §2).
#[must_use]
pub fn is_valid_lan_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=LAN_NAME_MAX_CHARS).contains(&count)
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

/// The highest version both sides speak.
#[must_use]
pub fn common_version(ours: &[u32], theirs: &[u32]) -> Option<u32> {
    ours.iter().filter(|v| theirs.contains(v)).max().copied()
}

fn bytes_of<const N: usize>(value: &Value) -> Result<[u8; N], HelloError> {
    match value {
        Value::Bytes(bytes) => bytes.as_slice().try_into().map_err(|_| HelloError),
        _ => Err(HelloError),
    }
}

fn uint(value: &Value) -> Result<u64, HelloError> {
    match value {
        Value::Integer(n) => u64::try_from(*n).map_err(|_| HelloError),
        _ => Err(HelloError),
    }
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

//! A device's identity in a sync group (design/06-sync.md §1.1).
//!
//! `DS` (Ed25519) is the long-term identity: the [`DeviceId`] is its public key. `DX` (X25519) is the static key
//! in Noise handshakes, bound to `DS` and to the group by a certificate. A new identity is made for every group
//! the device joins.

use std::fmt::{self, Write as _};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use keep_vault::{DeviceId, DeviceSecrets, DeviceTag};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// Domain separation for the DX certificate.
const DX_CERT_CONTEXT: &[u8] = b"otpkeep dx v1";
const TAG_LEN: usize = 8;
const SHORT_FINGERPRINT_LEN: usize = 2;
const FULL_FINGERPRINT_LEN: usize = 10;
/// Hex digits between dashes in the full fingerprint.
const FINGERPRINT_GROUP: usize = 4;

/// Why an identity could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    /// No random bytes for new keys.
    #[error("the system random number generator is unavailable")]
    RandomUnavailable,
}

/// Keys made for joining a group whose id is not known yet: the joiner needs them for the handshake and
/// certifies them once the group is known.
pub struct NewDevice {
    signing_seed: Zeroizing<[u8; 32]>,
    exchange: StaticSecret,
}

impl NewDevice {
    /// New random keys.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::RandomUnavailable`] when the system has no random bytes.
    pub fn generate() -> Result<Self, IdentityError> {
        let mut signing_seed = Zeroizing::new([0u8; 32]);
        let mut exchange = Zeroizing::new([0u8; 32]);
        getrandom::fill(signing_seed.as_mut()).map_err(|_| IdentityError::RandomUnavailable)?;
        getrandom::fill(exchange.as_mut()).map_err(|_| IdentityError::RandomUnavailable)?;
        Ok(Self {
            signing_seed,
            exchange: StaticSecret::from(*exchange),
        })
    }

    /// The device id these keys give.
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        DeviceId(
            SigningKey::from_bytes(&self.signing_seed)
                .verifying_key()
                .to_bytes(),
        )
    }

    /// The X25519 secret, for the handshake.
    #[must_use]
    pub fn exchange_secret(&self) -> &StaticSecret {
        &self.exchange
    }

    /// The identity in `group_id`, with its certificate.
    #[must_use]
    pub fn into_identity(self, group_id: &[u8; 16]) -> DeviceIdentity {
        DeviceIdentity::from_seeds(&self.signing_seed, &self.exchange.to_bytes(), group_id)
    }
}

impl fmt::Debug for NewDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NewDevice({})", short_fingerprint(self.device_id()))
    }
}

/// This device's keys in one sync group.
pub struct DeviceIdentity {
    signing: SigningKey,
    exchange: StaticSecret,
    dx_cert: [u8; 64],
}

impl DeviceIdentity {
    /// New random keys for `group_id`.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError::RandomUnavailable`] when the system has no random bytes.
    pub fn generate(group_id: &[u8; 16]) -> Result<Self, IdentityError> {
        let mut seeds = Zeroizing::new([0u8; 64]);
        getrandom::fill(&mut *seeds).map_err(|_| IdentityError::RandomUnavailable)?;
        let (signing, exchange) = seeds.split_at(32);
        let signing: &[u8; 32] = signing
            .try_into()
            .map_err(|_| IdentityError::RandomUnavailable)?;
        let exchange: &[u8; 32] = exchange
            .try_into()
            .map_err(|_| IdentityError::RandomUnavailable)?;
        Ok(Self::from_seeds(signing, exchange, group_id))
    }

    /// The identity kept in the vault.
    #[must_use]
    pub fn from_secrets(secrets: &DeviceSecrets, group_id: &[u8; 16]) -> Self {
        Self::from_seeds(&secrets.signing_seed, &secrets.exchange_secret, group_id)
    }

    /// The identity with these keys. Ed25519 signatures are deterministic, so the certificate is the same every
    /// time.
    #[must_use]
    pub fn from_seeds(
        signing_seed: &[u8; 32],
        exchange_secret: &[u8; 32],
        group_id: &[u8; 16],
    ) -> Self {
        let signing = SigningKey::from_bytes(signing_seed);
        let exchange = StaticSecret::from(*exchange_secret);
        let dx_public = PublicKey::from(&exchange).to_bytes();
        let dx_cert = signing
            .sign(&dx_cert_message(group_id, &dx_public))
            .to_bytes();
        Self {
            signing,
            exchange,
            dx_cert,
        }
    }

    /// The keys to store in the vault.
    #[must_use]
    pub fn secrets(&self) -> DeviceSecrets {
        DeviceSecrets {
            signing_seed: Zeroizing::new(self.signing.to_bytes()),
            exchange_secret: Zeroizing::new(self.exchange.to_bytes()),
        }
    }

    /// The public signing key, which names this device.
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        DeviceId(self.signing.verifying_key().to_bytes())
    }

    /// The X25519 public key used in handshakes.
    #[must_use]
    pub fn dx_public(&self) -> [u8; 32] {
        PublicKey::from(&self.exchange).to_bytes()
    }

    /// The signature binding [`Self::dx_public`] to this device and the group.
    #[must_use]
    pub fn dx_cert(&self) -> [u8; 64] {
        self.dx_cert
    }

    /// The X25519 secret, for the handshake.
    #[must_use]
    pub fn exchange_secret(&self) -> &StaticSecret {
        &self.exchange
    }

    /// Signs `message` with `DS`.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing.sign(message).to_bytes()
    }
}

impl fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceIdentity({})", short_fingerprint(self.device_id()))
    }
}

/// Whether `signature` is `device`'s signature of `message`. Uses strict verification, which rejects
/// malleable signatures and weak keys.
#[must_use]
pub fn verify(device: DeviceId, message: &[u8], signature: &[u8; 64]) -> bool {
    VerifyingKey::from_bytes(&device.0).is_ok_and(|key| {
        key.verify_strict(message, &Signature::from_bytes(signature))
            .is_ok()
    })
}

/// Whether `dx_cert` binds `dx_public` to `device` in `group_id`.
#[must_use]
pub fn verify_dx_cert(
    device: DeviceId,
    group_id: &[u8; 16],
    dx_public: &[u8; 32],
    dx_cert: &[u8; 64],
) -> bool {
    verify(device, &dx_cert_message(group_id, dx_public), dx_cert)
}

fn dx_cert_message(group_id: &[u8; 16], dx_public: &[u8; 32]) -> Vec<u8> {
    [DX_CERT_CONTEXT, group_id, dx_public].concat()
}

/// The device part of this device's timestamps.
#[must_use]
pub fn device_tag(device: DeviceId) -> DeviceTag {
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&id_hash(device)[..TAG_LEN]);
    DeviceTag(tag)
}

/// Four hex digits that tell same-named devices apart in lists, such as `7F3A`.
#[must_use]
pub fn short_fingerprint(device: DeviceId) -> String {
    hex(&id_hash(device)[..SHORT_FINGERPRINT_LEN])
}

/// Twenty hex digits in groups of four, compared by people before allowing a sync, such as
/// `7F3A-91C2-0B5E-44D8-A1F0`.
#[must_use]
pub fn full_fingerprint(device: DeviceId) -> String {
    let digits = hex(&id_hash(device)[..FULL_FINGERPRINT_LEN]);
    digits
        .as_bytes()
        .chunks(FINGERPRINT_GROUP)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

fn id_hash(device: DeviceId) -> [u8; 32] {
    *blake3::hash(&device.0).as_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, b| {
        // Writing to a String cannot fail.
        let _ = write!(text, "{b:02X}");
        text
    })
}

#[cfg(test)]
mod tests;

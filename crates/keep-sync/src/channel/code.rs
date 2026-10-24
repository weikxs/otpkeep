//! Checking the pairing or sync code inside the channel (design/06-sync.md §4.3 ②, §5.2).
//!
//! SPAKE2 turns the code into a shared key that a recording cannot be checked against offline. The key
//! confirmation is derived with the Noise handshake hash as salt, so a relay running a separate channel with
//! each side fails it. The joiner confirms first; the opener answers only when the joiner's confirmation is
//! right, and otherwise reports a wrong code, which the opener's window counts.

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::io::{AsyncRead, AsyncWrite};
use zeroize::Zeroizing;

use super::message::{ErrorCode, Message};
use super::noise::Channel;

const MODE_PAIR_SECRET: u8 = 1;
const MODE_PAIR_CODE: u8 = 2;
const MODE_SYNC_CODE: u8 = 3;
const SAS_DIGITS: u64 = 1_000_000;

/// The secret one side knows.
#[derive(Clone, PartialEq, Eq)]
pub enum Secret {
    /// The 128-bit pairing secret from the QR code.
    PairSecret([u8; 16]),
    /// The 8-digit pairing code.
    PairCode(Zeroizing<String>),
    /// The 6-digit sync code.
    SyncCode(Zeroizing<String>),
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl Secret {
    fn mode(&self) -> u8 {
        match self {
            Self::PairSecret(_) => MODE_PAIR_SECRET,
            Self::PairCode(_) => MODE_PAIR_CODE,
            Self::SyncCode(_) => MODE_SYNC_CODE,
        }
    }

    fn is_pairing(&self) -> bool {
        !matches!(self, Self::SyncCode(_))
    }

    /// The SPAKE2 password: the secret's kind, then the secret.
    fn password(&self) -> Zeroizing<Vec<u8>> {
        let (label, value): (&[u8], &[u8]) = match self {
            Self::PairSecret(secret) => (b"pair secret:", secret),
            Self::PairCode(code) => (b"pair code:", code.as_bytes()),
            Self::SyncCode(code) => (b"sync code:", code.as_bytes()),
        };
        Zeroizing::new([label, value].concat())
    }
}

/// Which side checks, with what it knows.
#[derive(Debug, Clone, Copy)]
pub enum CodeCheck<'a> {
    /// The joining side, with what the person typed or scanned.
    Joiner(&'a Secret),
    /// The opener of a sync window.
    Opener(&'a Secret),
    /// The opener of a pairing window, which accepts the QR code's secret or the typed code.
    PairOpener {
        /// The secret in the QR code.
        secret: &'a [u8; 16],
        /// The code on the screen.
        code: &'a str,
    },
}

/// Why the code check failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    /// The codes differ, or something sat between the two devices (`CODE_WRONG`).
    #[error("the code is not correct")]
    Wrong,
    /// The other side stopped with this code.
    #[error("the other device stopped: {}", .0.as_str())]
    Stopped(ErrorCode),
    /// The other side broke the protocol or the connection.
    #[error("the connection failed during the code check")]
    Broken,
}

/// A passed code check.
#[derive(Clone, PartialEq, Eq)]
pub struct Confirmed {
    /// The six-digit check code both people compare when pairing.
    pub sas: String,
}

impl std::fmt::Debug for Confirmed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Confirmed(…)")
    }
}

/// Runs the code check on an established channel.
///
/// # Errors
///
/// Returns [`CodeError`]. On the opener, [`CodeError::Wrong`] is to be counted by the window.
pub async fn confirm_code<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    check: CodeCheck<'_>,
) -> Result<Confirmed, CodeError> {
    match check {
        CodeCheck::Joiner(secret) => joiner(channel, secret).await,
        CodeCheck::Opener(secret) => {
            opener(channel, |mode| {
                (mode == secret.mode()).then(|| secret.clone())
            })
            .await
        }
        CodeCheck::PairOpener { secret, code } => {
            opener(channel, |mode| match mode {
                MODE_PAIR_SECRET => Some(Secret::PairSecret(*secret)),
                MODE_PAIR_CODE => Some(Secret::PairCode(Zeroizing::new(code.to_owned()))),
                _ => None,
            })
            .await
        }
    }
}

async fn joiner<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    secret: &Secret,
) -> Result<Confirmed, CodeError> {
    let (initiator, responder) = identities(secret.is_pairing());
    let (state, outbound) = Spake2::<Ed25519Group>::start_a(
        &Password::new(secret.password().as_slice()),
        &Identity::new(initiator),
        &Identity::new(responder),
    );
    send(
        channel,
        &Message::CodeStart {
            mode: secret.mode(),
            spake: outbound,
        },
    )
    .await?;
    let reply = match receive(channel).await? {
        Message::CodeReply { spake } => spake,
        other => return Err(unexpected(&other)),
    };
    let key = Zeroizing::new(state.finish(&reply).map_err(|_| CodeError::Broken)?);
    let keys = Keys::derive(&key, &channel.handshake_hash(), secret.is_pairing())?;
    send(
        channel,
        &Message::Confirm {
            mac: keys.mac(INITIATOR_LABEL)?,
        },
    )
    .await?;
    match receive(channel).await? {
        Message::Confirm { mac } if keys.verify(RESPONDER_LABEL, &mac) => Ok(keys.confirmed()),
        Message::Confirm { .. } | Message::Error(ErrorCode::CodeWrong) => Err(CodeError::Wrong),
        other => Err(unexpected(&other)),
    }
}

async fn opener<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    secret_for: impl Fn(u8) -> Option<Secret>,
) -> Result<Confirmed, CodeError> {
    let (mode, inbound) = match receive(channel).await? {
        Message::CodeStart { mode, spake } => (mode, spake),
        other => return Err(unexpected(&other)),
    };
    let Some(secret) = secret_for(mode) else {
        let _ = send(channel, &Message::Error(ErrorCode::Protocol)).await;
        return Err(CodeError::Broken);
    };
    let (initiator, responder) = identities(secret.is_pairing());
    let (state, outbound) = Spake2::<Ed25519Group>::start_b(
        &Password::new(secret.password().as_slice()),
        &Identity::new(initiator),
        &Identity::new(responder),
    );
    send(channel, &Message::CodeReply { spake: outbound }).await?;
    let key = Zeroizing::new(state.finish(&inbound).map_err(|_| CodeError::Broken)?);
    let keys = Keys::derive(&key, &channel.handshake_hash(), secret.is_pairing())?;
    match receive(channel).await? {
        Message::Confirm { mac } if keys.verify(INITIATOR_LABEL, &mac) => {
            send(
                channel,
                &Message::Confirm {
                    mac: keys.mac(RESPONDER_LABEL)?,
                },
            )
            .await?;
            Ok(keys.confirmed())
        }
        Message::Confirm { .. } => {
            let _ = send(channel, &Message::Error(ErrorCode::CodeWrong)).await;
            Err(CodeError::Wrong)
        }
        other => Err(unexpected(&other)),
    }
}

const INITIATOR_LABEL: &[u8] = b"initiator";
const RESPONDER_LABEL: &[u8] = b"responder";

fn identities(pairing: bool) -> (&'static [u8], &'static [u8]) {
    if pairing {
        (b"otpkeep pair initiator", b"otpkeep pair responder")
    } else {
        (b"otpkeep sync initiator", b"otpkeep sync responder")
    }
}

/// The confirmation key and the check code, both bound to the handshake.
struct Keys {
    confirm: Zeroizing<[u8; 32]>,
    sas: String,
}

impl Keys {
    fn derive(key: &[u8], handshake_hash: &[u8; 32], pairing: bool) -> Result<Self, CodeError> {
        let hkdf = Hkdf::<Sha256>::new(Some(handshake_hash), key);
        let info: &[u8] = if pairing {
            b"otpkeep pair confirm"
        } else {
            b"otpkeep sync confirm"
        };
        let mut confirm = Zeroizing::new([0u8; 32]);
        hkdf.expand(info, confirm.as_mut())
            .map_err(|_| CodeError::Broken)?;
        let mut sas = [0u8; 8];
        hkdf.expand(b"otpkeep sas", &mut sas)
            .map_err(|_| CodeError::Broken)?;
        Ok(Self {
            confirm,
            sas: format!("{:06}", u64::from_be_bytes(sas) % SAS_DIGITS),
        })
    }

    fn mac(&self, label: &[u8]) -> Result<[u8; 32], CodeError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.confirm.as_ref()).map_err(|_| CodeError::Broken)?;
        mac.update(label);
        Ok(mac.finalize().into_bytes().into())
    }

    /// Constant-time comparison.
    fn verify(&self, label: &[u8], tag: &[u8; 32]) -> bool {
        Hmac::<Sha256>::new_from_slice(self.confirm.as_ref()).is_ok_and(|mut mac| {
            mac.update(label);
            mac.verify_slice(tag).is_ok()
        })
    }

    fn confirmed(self) -> Confirmed {
        Confirmed { sas: self.sas }
    }
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    message: &Message,
) -> Result<(), CodeError> {
    channel
        .send(&message.encode())
        .await
        .map_err(|_| CodeError::Broken)
}

async fn receive<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<Message, CodeError> {
    let bytes = channel.recv().await.map_err(|_| CodeError::Broken)?;
    Message::decode(&bytes).ok_or(CodeError::Broken)
}

fn unexpected(message: &Message) -> CodeError {
    match message {
        Message::Error(ErrorCode::CodeWrong) => CodeError::Wrong,
        Message::Error(code) => CodeError::Stopped(*code),
        _ => CodeError::Broken,
    }
}

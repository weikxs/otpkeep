//! The Noise XX handshake and the encrypted channel built from it.
//!
//! On the wire every Noise message is a frame: a two-byte big-endian length and the bytes. An application
//! message is a four-byte length followed by its bytes, cut into as many encrypted frames as it needs.

use snow::{Builder, HandshakeState, TransportState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use x25519_dalek::StaticSecret;
use zeroize::Zeroizing;

use super::hello::{Hello, HelloError};
use super::{MAX_MESSAGE, MAX_SESSION};

const NOISE_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
/// The largest Noise message.
const MAX_FRAME: usize = 65_535;
/// Encryption adds this many bytes to each frame.
const TAG_LEN: usize = 16;
const LENGTH_PREFIX: usize = 4;

/// What a connection is for; part of the prologue, so the two kinds never mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Adding a device.
    Pair,
    /// Syncing with a member.
    Sync,
}

impl Purpose {
    fn prologue(self, window_random: &[u8; 16]) -> Vec<u8> {
        let label: &[u8] = match self {
            Self::Pair => b"otpkeep/1 pair",
            Self::Sync => b"otpkeep/1 sync",
        };
        [label, window_random].concat()
    }
}

/// Why the channel failed. The connection is closed afterwards.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// The connection broke or closed.
    #[error("the connection was lost")]
    Io(#[from] std::io::Error),
    /// The handshake failed: another window, another kind, or a broken peer.
    #[error("the secure connection could not be set up")]
    Handshake,
    /// A frame could not be decrypted: it was changed on the way.
    #[error("the connection was tampered with")]
    Tampered,
    /// The other side's introduction was not readable.
    #[error("the other device sent an unreadable introduction")]
    Hello(#[from] HelloError),
    /// A message is larger than allowed (`TOO_LARGE`).
    #[error("the data is larger than one sync allows")]
    TooLarge,
    /// The opener is busy with another device (`BUSY`).
    #[error("the other device is busy with another connection")]
    Busy,
    /// Nothing arrived in time.
    #[error("the other device stopped responding")]
    TimedOut,
}

/// Tells a connection that arrived while another is handled that the window is busy, before any handshake:
/// an empty frame in place of the handshake's second message, which no Noise message can be. The caller limits
/// how long this may take.
///
/// # Errors
///
/// Returns the connection's error.
pub async fn refuse_busy<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) -> std::io::Result<()> {
    // The joiner speaks first; answer its first message, so it reads the reply rather than a reset.
    let len = stream.read_u16().await?;
    let mut first = vec![0u8; usize::from(len)];
    stream.read_exact(&mut first).await?;
    stream.write_u16(0).await?;
    stream.flush().await?;
    stream.shutdown().await
}

/// An encrypted, authenticated connection.
pub struct Channel<S> {
    stream: S,
    transport: TransportState,
    handshake_hash: [u8; 32],
    remote_static: [u8; 32],
    received: usize,
    idle: Option<std::time::Duration>,
}

/// Runs the opener's side of the handshake on an accepted connection. Returns the channel and the joiner's
/// introduction; whether the joiner may continue is for the caller to decide.
///
/// # Errors
///
/// Returns [`ChannelError`].
pub async fn accept_noise<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    purpose: Purpose,
    window_random: &[u8; 16],
    exchange: &StaticSecret,
    ours: &Hello,
) -> Result<(Channel<S>, Hello), ChannelError> {
    let key = Zeroizing::new(exchange.to_bytes());
    let prologue = purpose.prologue(window_random);
    let mut state = builder(&key, &prologue)?
        .build_responder()
        .map_err(|_| ChannelError::Handshake)?;
    read_handshake(&mut stream, &mut state).await?;
    write_handshake(&mut stream, &mut state, &ours.encode()).await?;
    let theirs = Hello::decode(&read_handshake(&mut stream, &mut state).await?)?;
    Ok((finish(stream, state)?, theirs))
}

/// Runs the joiner's side of the handshake. Returns the channel and the opener's introduction.
///
/// # Errors
///
/// Returns [`ChannelError`].
pub async fn connect_noise<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    purpose: Purpose,
    window_random: &[u8; 16],
    exchange: &StaticSecret,
    ours: &Hello,
) -> Result<(Channel<S>, Hello), ChannelError> {
    let key = Zeroizing::new(exchange.to_bytes());
    let prologue = purpose.prologue(window_random);
    let mut state = builder(&key, &prologue)?
        .build_initiator()
        .map_err(|_| ChannelError::Handshake)?;
    write_handshake(&mut stream, &mut state, &[]).await?;
    let reply = read_frame(&mut stream).await?;
    if reply.is_empty() {
        return Err(ChannelError::Busy);
    }
    let mut payload = vec![0u8; MAX_FRAME];
    let len = state
        .read_message(&reply, &mut payload)
        .map_err(|_| ChannelError::Handshake)?;
    let theirs = Hello::decode(&payload[..len])?;
    write_handshake(&mut stream, &mut state, &ours.encode()).await?;
    Ok((finish(stream, state)?, theirs))
}

fn builder<'a>(key: &'a [u8; 32], prologue: &'a [u8]) -> Result<Builder<'a>, ChannelError> {
    let params = NOISE_PARAMS.parse().map_err(|_| ChannelError::Handshake)?;
    Builder::new(params)
        .local_private_key(key)
        .and_then(|b| b.prologue(prologue))
        .map_err(|_| ChannelError::Handshake)
}

async fn write_handshake<S: AsyncWrite + Unpin>(
    stream: &mut S,
    state: &mut HandshakeState,
    payload: &[u8],
) -> Result<(), ChannelError> {
    let mut frame = vec![0u8; MAX_FRAME];
    let len = state
        .write_message(payload, &mut frame)
        .map_err(|_| ChannelError::Handshake)?;
    write_frame(stream, &frame[..len]).await
}

async fn read_handshake<S: AsyncRead + Unpin>(
    stream: &mut S,
    state: &mut HandshakeState,
) -> Result<Vec<u8>, ChannelError> {
    let frame = read_frame(stream).await?;
    let mut payload = vec![0u8; MAX_FRAME];
    let len = state
        .read_message(&frame, &mut payload)
        .map_err(|_| ChannelError::Handshake)?;
    payload.truncate(len);
    Ok(payload)
}

fn finish<S>(stream: S, state: HandshakeState) -> Result<Channel<S>, ChannelError> {
    let handshake_hash: [u8; 32] = state
        .get_handshake_hash()
        .try_into()
        .map_err(|_| ChannelError::Handshake)?;
    let remote_static: [u8; 32] = state
        .get_remote_static()
        .and_then(|key| key.try_into().ok())
        .ok_or(ChannelError::Handshake)?;
    let transport = state
        .into_transport_mode()
        .map_err(|_| ChannelError::Handshake)?;
    Ok(Channel {
        stream,
        transport,
        handshake_hash,
        remote_static,
        received: 0,
        idle: None,
    })
}

async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    frame: &[u8],
) -> Result<(), ChannelError> {
    let len = u16::try_from(frame.len()).map_err(|_| ChannelError::TooLarge)?;
    stream.write_u16(len).await?;
    stream.write_all(frame).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, ChannelError> {
    let len = stream.read_u16().await?;
    let mut frame = vec![0u8; usize::from(len)];
    stream.read_exact(&mut frame).await?;
    Ok(frame)
}

impl<S: AsyncRead + AsyncWrite + Unpin> Channel<S> {
    /// The Noise handshake hash, which the code check binds to.
    #[must_use]
    pub fn handshake_hash(&self) -> [u8; 32] {
        self.handshake_hash
    }

    /// The other side's X25519 key, proved by the handshake.
    #[must_use]
    pub fn remote_static(&self) -> [u8; 32] {
        self.remote_static
    }

    /// Ends the connection after a final message: stops sending, then reads and discards what the other side
    /// still sends until it closes or `linger` passes. Closing with unread data would make the system reset the
    /// connection, and the other side could lose the final message.
    pub async fn close(&mut self, linger: std::time::Duration) {
        let _ = self.stream.shutdown().await;
        let mut sink = [0u8; 4096];
        let drain = async {
            while let Ok(read) = self.stream.read(&mut sink).await {
                if read == 0 {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(linger, drain).await;
    }

    /// From now on, a receive that waits longer than `idle` fails with [`ChannelError::TimedOut`].
    pub fn set_idle_timeout(&mut self, idle: Option<std::time::Duration>) {
        self.idle = idle;
    }

    /// Sends one message of at most [`MAX_MESSAGE`] bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::TooLarge`] or a connection error.
    pub async fn send(&mut self, message: &[u8]) -> Result<(), ChannelError> {
        if message.len() > MAX_MESSAGE {
            return Err(ChannelError::TooLarge);
        }
        let len = u32::try_from(message.len()).map_err(|_| ChannelError::TooLarge)?;
        let mut plain = Zeroizing::new(Vec::with_capacity(LENGTH_PREFIX + message.len()));
        plain.extend_from_slice(&len.to_be_bytes());
        plain.extend_from_slice(message);
        let mut frame = vec![0u8; MAX_FRAME];
        for chunk in plain.chunks(MAX_FRAME - TAG_LEN) {
            let written = self
                .transport
                .write_message(chunk, &mut frame)
                .map_err(|_| ChannelError::Handshake)?;
            write_frame(&mut self.stream, &frame[..written]).await?;
        }
        Ok(())
    }

    /// Receives one message. A session may receive at most [`MAX_SESSION`] bytes in all.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Tampered`] for a changed frame, [`ChannelError::TooLarge`], or a connection
    /// error.
    pub async fn recv(&mut self) -> Result<Zeroizing<Vec<u8>>, ChannelError> {
        let mut message = Zeroizing::new(Vec::new());
        let mut first = self.decrypt_frame().await?;
        if first.len() < LENGTH_PREFIX {
            return Err(ChannelError::Tampered);
        }
        let mut len = [0u8; LENGTH_PREFIX];
        len.copy_from_slice(&first[..LENGTH_PREFIX]);
        let total = usize::try_from(u32::from_be_bytes(len)).map_err(|_| ChannelError::TooLarge)?;
        if total > MAX_MESSAGE || self.received.saturating_add(total) > MAX_SESSION {
            return Err(ChannelError::TooLarge);
        }
        message.extend_from_slice(&first[LENGTH_PREFIX..]);
        first.clear();
        while message.len() < total {
            let next = self.decrypt_frame().await?;
            message.extend_from_slice(&next);
        }
        if message.len() != total {
            return Err(ChannelError::Tampered);
        }
        self.received += total;
        Ok(message)
    }

    async fn decrypt_frame(&mut self) -> Result<Zeroizing<Vec<u8>>, ChannelError> {
        let frame = match self.idle {
            Some(idle) => tokio::time::timeout(idle, read_frame(&mut self.stream))
                .await
                .map_err(|_| ChannelError::TimedOut)??,
            None => read_frame(&mut self.stream).await?,
        };
        let mut plain = Zeroizing::new(vec![0u8; frame.len()]);
        let len = self
            .transport
            .read_message(&frame, &mut plain)
            .map_err(|_| ChannelError::Tampered)?;
        plain.truncate(len);
        Ok(plain)
    }
}

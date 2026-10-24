//! A byte stream over GATT, for platforms without L2CAP channels (Windows; design/06-sync.md §3.6).
//!
//! The joining side writes (without response) to one characteristic and the opener notifies on another. The
//! link layer delivers both in order and the system queues tell the sender when to wait, so a packet is only
//! a 2-byte header — kind and sequence number — and up to `payload_max` bytes. A gap in the sequence numbers
//! (a packet lost by a stack) ends the stream instead of handing on damaged data; the Noise layer above would
//! reject it anyway.

use std::future::Future;
use std::io;
use std::pin::Pin;

use keep_sync::transport::BoxLink;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// Bytes of each packet's header.
pub const HEADER: usize = 2;
/// The smallest payload that makes progress (a 23-byte ATT MTU leaves 20 bytes for a value).
pub const MIN_PAYLOAD: usize = 20 - HEADER;
const KIND_DATA: u8 = 1;
const KIND_CLOSE: u8 = 2;
const PIPE: usize = 64 * 1024;

/// One packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// Stream bytes.
    Data {
        /// Wrapping sequence number.
        seq: u8,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The sender closed the stream.
    Close {
        /// Wrapping sequence number.
        seq: u8,
    },
}

impl Packet {
    /// The packet as written to the characteristic.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Data { seq, bytes } => [&[KIND_DATA, *seq][..], bytes].concat(),
            Self::Close { seq } => vec![KIND_CLOSE, *seq],
        }
    }

    /// Reads a packet; `None` for anything malformed.
    #[must_use]
    pub fn decode(value: &[u8]) -> Option<Self> {
        match value {
            [KIND_DATA, seq, bytes @ ..] if !bytes.is_empty() => Some(Self::Data {
                seq: *seq,
                bytes: bytes.to_vec(),
            }),
            [KIND_CLOSE, seq] => Some(Self::Close { seq: *seq }),
            _ => None,
        }
    }

    fn seq(&self) -> u8 {
        match self {
            Self::Data { seq, .. } | Self::Close { seq } => *seq,
        }
    }
}

/// The future a [`PacketSink`] returns.
pub type Sending<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

/// Sends packets to the other side: a notification or a write without response. It returns once the system
/// accepted the packet, waiting while its queue is full.
pub trait PacketSink: Send + 'static {
    /// Sends one encoded packet.
    fn send(&mut self, packet: Vec<u8>) -> Sending<'_>;
}

/// A byte stream over packets: `sink` carries what the session writes, `incoming` brings the other side's
/// packets (as received from the characteristic). Call from within a Tokio runtime.
pub fn gatt_link(
    mut sink: impl PacketSink,
    mut incoming: mpsc::Receiver<Vec<u8>>,
    payload_max: usize,
) -> BoxLink {
    let payload_max = payload_max.max(MIN_PAYLOAD);
    let (session_end, pipe_end) = tokio::io::duplex(PIPE);
    let (mut read_half, mut write_half) = tokio::io::split(pipe_end);
    tokio::spawn(async move {
        let mut buffer = vec![0u8; payload_max];
        let mut seq: u8 = 0;
        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let packet = Packet::Data {
                        seq,
                        bytes: buffer[..n].to_vec(),
                    };
                    seq = seq.wrapping_add(1);
                    if sink.send(packet.encode()).await.is_err() {
                        return;
                    }
                }
            }
        }
        let _ = sink.send(Packet::Close { seq }.encode()).await;
    });
    tokio::spawn(async move {
        let mut expected: u8 = 0;
        while let Some(value) = incoming.recv().await {
            let Some(packet) = Packet::decode(&value) else {
                tracing::warn!("a malformed packet ended the Bluetooth stream");
                break;
            };
            if packet.seq() != expected {
                tracing::warn!("a lost packet ended the Bluetooth stream");
                break;
            }
            expected = expected.wrapping_add(1);
            match packet {
                Packet::Data { bytes, .. } => {
                    if write_half.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Packet::Close { .. } => break,
            }
        }
        let _ = write_half.shutdown().await;
    });
    Box::new(session_end)
}

#[cfg(test)]
mod tests;

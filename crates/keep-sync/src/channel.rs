//! The encrypted channel between two devices (design/06-sync.md §4.3, §5.4).
//!
//! A Noise XX handshake (`Noise_XX_25519_ChaChaPoly_BLAKE2s`) with the window's random value in the prologue
//! authenticates each side's exchange key and encrypts everything after it. The pairing or sync code is then
//! checked inside the channel with SPAKE2, and its confirmation is bound to the Noise handshake hash, so a relay
//! that runs separate channels with each side cannot pass the check and a recording allows no offline guessing.

pub mod code;
pub mod hello;
pub mod message;
pub mod noise;

/// The protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// `SYNC_MAX_MESSAGE`: the largest message (design/10-reference.md §2).
pub const MAX_MESSAGE: usize = 1024 * 1024;
/// `SYNC_MAX_SESSION`: the most data one session may receive.
pub const MAX_SESSION: usize = 64 * 1024 * 1024;

#[cfg(test)]
mod tests;

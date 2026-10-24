//! Bluetooth Low Energy for pairing and sync (design/06-sync.md §3.6, design/07-platform.md §5.1, ADR-017).
//!
//! An open window advertises only OTPKeep's service id; its window information (the same record as on the
//! local network, plus the L2CAP channel) is read after connecting. Sessions then run over an L2CAP
//! connection-oriented channel without system pairing: all security is in the protocol. This crate only finds
//! windows and provides byte streams; it never sees keys or account data.
//!
//! Unsafe code is allowed only in the platform bindings, each use with its reason (ADR-018).

use std::time::Duration;

use keep_sync::lan::announce::{Announcement, FoundWindow};
use keep_sync::transport::Incoming;
use tokio::sync::mpsc;

pub mod gatt;
#[cfg(target_os = "macos")]
mod macos;

/// OTPKeep's Bluetooth service (design/10-reference.md §2).
pub const SERVICE_UUID: &str = "02A695FD-A1E3-485D-A399-0DFB568A936C";
/// The readable "window information" characteristic.
pub const WINDOW_INFO_UUID: &str = "02A695FD-A1E3-485D-A399-0DFB568A936D";

/// Why Bluetooth cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BleError {
    /// Bluetooth is turned off (`BLUETOOTH_OFF`).
    #[error("Bluetooth is turned off")]
    Off,
    /// This program may not use Bluetooth (`BLUETOOTH_DENIED`).
    #[error("this program is not allowed to use Bluetooth; allow it in the system settings")]
    Denied,
    /// This device or system cannot do what is needed (`BLUETOOTH_UNSUPPORTED`).
    #[error("this device does not support the Bluetooth features OTPKeep needs")]
    Unsupported,
    /// The system reported an error.
    #[error("Bluetooth failed: {0}")]
    Failed(String),
}

impl BleError {
    /// The stable code (design/10-reference.md §4.1).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Off => "BLUETOOTH_OFF",
            Self::Denied => "BLUETOOTH_DENIED",
            Self::Unsupported | Self::Failed(_) => "BLUETOOTH_UNSUPPORTED",
        }
    }
}

/// A window announced over Bluetooth; it stops when dropped.
pub struct BleWindow {
    #[cfg(target_os = "macos")]
    _inner: macos::peripheral::Peripheral,
}

impl BleWindow {
    /// Announces `announcement` and hands every incoming connection to `connections`.
    ///
    /// # Errors
    ///
    /// Returns [`BleError`] when Bluetooth cannot be used.
    #[allow(
        clippy::unused_async,
        reason = "platforms without Bluetooth support return at once"
    )]
    pub async fn open(
        announcement: &Announcement,
        connections: mpsc::Sender<Incoming>,
    ) -> Result<Self, BleError> {
        #[cfg(target_os = "macos")]
        {
            let inner = macos::peripheral::Peripheral::open(announcement, connections).await?;
            Ok(Self { _inner: inner })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (announcement, connections);
            Err(BleError::Unsupported)
        }
    }
}

/// Searches for windows for up to `limit`, stopping `settle` after the first one is found.
///
/// # Errors
///
/// Returns [`BleError`] when Bluetooth cannot be used.
#[allow(
    clippy::unused_async,
    reason = "platforms without Bluetooth support return at once"
)]
pub async fn browse(limit: Duration, settle: Duration) -> Result<Vec<FoundWindow>, BleError> {
    #[cfg(target_os = "macos")]
    {
        macos::central::browse(limit, settle).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (limit, settle);
        Err(BleError::Unsupported)
    }
}

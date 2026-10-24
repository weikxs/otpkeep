//! Reaching another device (design/06-sync.md §3): opening a pairing or sync window on the local network and
//! over Bluetooth, finding the windows other devices opened, and introducing this device. Shared by the
//! command line and the app; the sessions themselves are in `keep_sync::flow`.

pub mod device;
pub mod network;

pub use network::{LinkError, Network, Opened, Skipped, Transport, WindowDirectory};

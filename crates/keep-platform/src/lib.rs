//! Traits for platform services: keystore, unlock, clipboard, LAN discovery and clock.

mod keystore;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use keystore::SystemKeystore;
pub use keystore::{Keystore, KeystoreError, MemoryKeystore};

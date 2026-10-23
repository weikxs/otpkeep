//! Traits for platform services: keystore, unlock, clipboard, LAN discovery and clock.

mod keystore;
mod paths;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use keystore::SystemKeystore;
pub use keystore::{Keystore, KeystoreError, MemoryKeystore};
pub use paths::{DATA_DIR_OVERRIDE, Os, data_dir, data_dir_for};

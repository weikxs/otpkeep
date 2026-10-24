//! The local network: which cards to use, discovery and windows (design/06-sync.md §3).

pub mod interfaces;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub mod system;
pub mod window;

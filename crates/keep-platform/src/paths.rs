//! Where each desktop platform keeps OTPKeep's data (design/02-data.md §3.1).

use std::path::PathBuf;

/// Environment variable that points OTPKeep at another data directory (tests use one under `/tmp`).
pub const DATA_DIR_OVERRIDE: &str = "OTPKEEP_DATA_DIR";

/// A desktop operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
    /// Linux and other Unix desktops.
    Linux,
}

impl Os {
    /// The system this program runs on.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The data directory on this system, from the process environment.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    data_dir_for(Os::current(), |name| std::env::var(name).ok())
}

/// Whether `path` is absolute by `os`'s rules, whichever system runs this (so every system can be tested
/// anywhere): `/…` on macOS and Linux; a drive (`C:\…`) or a network path (`\\server\…`) on Windows.
fn is_absolute_on(os: Os, path: &str) -> bool {
    match os {
        Os::MacOs | Os::Linux => path.starts_with('/'),
        Os::Windows => {
            let bytes = path.as_bytes();
            let drive = bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'\\' | b'/');
            drive || path.starts_with(r"\\")
        }
    }
}

/// The data directory on `os`, reading the environment through `var`.
///
/// Windows uses the local, not the roaming, application data: the data key does not roam with the profile, so
/// a roamed vault could not be opened anyway.
pub fn data_dir_for(os: Os, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let absolute = |name: &str| {
        var(name)
            .filter(|v| is_absolute_on(os, v))
            .map(PathBuf::from)
    };
    if let Some(dir) = absolute(DATA_DIR_OVERRIDE) {
        return Some(dir);
    }
    match os {
        Os::MacOs => absolute("HOME").map(|home| home.join("Library/Application Support/OTPKeep")),
        Os::Windows => var("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .map(|local| PathBuf::from(local).join("OTPKeep")),
        Os::Linux => absolute("XDG_DATA_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local/share")))
            .map(|data| data.join("otpkeep")),
    }
}

#[cfg(test)]
mod tests;

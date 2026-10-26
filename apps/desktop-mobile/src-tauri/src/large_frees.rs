//! Returning large freed memory to the system at once (macOS).
//!
//! Deriving the key from the master password (Argon2id) needs 64 MiB for a moment. macOS keeps freed large
//! blocks in a cache for reuse, and that cache counts in the app's memory until the system reclaims it, which
//! can take minutes: after unlocking, the app resting in the menu bar showed 78 MB instead of 14 MB. The cache
//! is turned off with the `MallocLargeCache=0` setting of the system allocator, which is read only when a
//! process starts, so the app starts itself again once with it, before anything else runs. Nothing else
//! changes: the same program, arguments and environment, and the same process (`exec`).

use std::ffi::OsStr;
use std::os::unix::process::CommandExt;

/// The system allocator's setting for its cache of large freed blocks.
const LARGE_CACHE: &str = "MallocLargeCache";

/// Starts this program again with the large-block cache off, unless that was done already. Returns only when
/// it was done already or could not be done; the app then runs as it is.
pub fn without_large_cache() {
    if !needs_restart(std::env::var_os(LARGE_CACHE).as_deref()) {
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    // `exec` returns only when it failed; the app then runs with the cache, as before.
    let _ = std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .env(LARGE_CACHE, "0")
        .exec();
}

/// Whether the process still has to be started with the cache off: only when nobody chose a setting.
fn needs_restart(setting: Option<&OsStr>) -> bool {
    setting.is_none()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::needs_restart;

    #[test]
    fn starts_again_only_once_and_never_over_a_chosen_setting() {
        assert!(needs_restart(None));
        assert!(!needs_restart(Some(OsStr::new("0"))));
        assert!(!needs_restart(Some(OsStr::new("1"))));
    }
}

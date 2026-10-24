//! Pairing, sync and the devices of the sync group from the command line (design/06-sync.md).
//!
//! The sessions run on a Tokio runtime; questions to the person (allow, check code, codes to type) are asked
//! on the calling thread while the session keeps running, so keepalives and timers continue meanwhile.

use keep_link::LinkError;
use keep_link::network::Opened;
use keep_sync::lan::announce::FoundWindow;
use keep_sync::transport::Via;
use keep_vault::Platform;

use crate::{CliError, EXIT_INVALID_INPUT, EXIT_NOT_SYNCED};

mod devices;
mod pair;
mod sync;

pub(crate) use devices::{devices, leave, remove_device, rename};
pub use keep_link::{Network, WindowDirectory};
pub(crate) use pair::{JoinFlags, pair_join, pair_open};
pub(crate) use sync::{sync_join, sync_wait};

impl From<LinkError> for CliError {
    fn from(error: LinkError) -> Self {
        not_synced(error.code, &error.message)
    }
}

/// What the person is told about transports a window does without.
pub(crate) fn skipped_notes(opened: &Opened) -> Vec<String> {
    opened
        .skipped
        .iter()
        .map(|s| match s.via {
            Via::Lan => format!("Local network not used: {}", s.error.message),
            Via::Bluetooth => format!("Bluetooth not used: {}", s.error.message),
        })
        .collect()
}

/// This device's platform, as other devices will show it.
pub(crate) const fn this_platform() -> Platform {
    keep_link::device::this_platform()
}

/// A platform's name as people know it.
pub(crate) fn platform_name(platform: Platform) -> String {
    match platform {
        Platform::MacOs => "macOS".to_owned(),
        Platform::Windows => "Windows".to_owned(),
        Platform::Linux => "Linux".to_owned(),
        Platform::Ios => "iOS".to_owned(),
        Platform::Android => "Android".to_owned(),
        Platform::Other(code) => format!("platform {code}"),
    }
}

/// A transport's name as people know it.
pub(crate) fn via_name(via: Via) -> &'static str {
    match via {
        Via::Lan => "local network",
        Via::Bluetooth => "Bluetooth",
    }
}

/// "1 entry", "2 entries".
pub(crate) fn entries(count: usize) -> String {
    if count == 1 {
        "1 entry".to_owned()
    } else {
        format!("{count} entries")
    }
}

/// A multi-threaded runtime, so sessions keep running while a question waits for the person.
pub(crate) fn runtime() -> Result<tokio::runtime::Runtime, CliError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| not_synced("RUNTIME", "the network runtime could not start"))
}

/// One window to use: the only one found, or the one the person picks.
pub(crate) fn pick<'a>(
    context: &crate::Context<'_>,
    windows: &'a [FoundWindow],
    to: Option<&str>,
) -> Result<&'a FoundWindow, CliError> {
    let matches: Vec<&FoundWindow> = windows
        .iter()
        .filter(|w| {
            to.is_none_or(|to| {
                w.announcement.short_fingerprint.eq_ignore_ascii_case(to)
                    || w.announcement
                        .lan_name
                        .to_lowercase()
                        .contains(&to.to_lowercase())
            })
        })
        .collect();
    match matches.as_slice() {
        [] => Err(not_synced(
            "NOT_FOUND",
            "no waiting device was found: keep the devices close with Bluetooth on, or on the same network (guest or public Wi-Fi may block devices from reaching each other)",
        )),
        [only] => Ok(only),
        many => {
            let list: Vec<String> = many
                .iter()
                .enumerate()
                .map(|(n, w)| {
                    format!(
                        "{}. {} · {} ({})",
                        n + 1,
                        w.announcement.lan_name,
                        w.announcement.short_fingerprint,
                        via_name(w.route.via())
                    )
                })
                .collect();
            let question = format!("{}\nWhich device (number)?", list.join("\n"));
            let answer = context
                .stdin_is_tty
                .then(|| (context.prompt)(crate::Prompt::Text(question)))
                .flatten()
                .ok_or_else(|| {
                    CliError::with_candidates(
                        EXIT_INVALID_INPUT,
                        "MULTIPLE_MATCHES",
                        "several devices are waiting; choose one with --to",
                        list.clone(),
                    )
                })?;
            answer
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|n| many.get(n.wrapping_sub(1)).copied())
                .ok_or_else(|| CliError::new(EXIT_INVALID_INPUT, "INVALID_INPUT", "no such device"))
        }
    }
}

/// A session or window that did not complete.
pub(crate) fn not_synced(code: &'static str, message: &str) -> CliError {
    CliError::new(EXIT_NOT_SYNCED, code, message)
}

/// The message and stable code for a session stop.
pub(crate) fn stopped(code: keep_sync::channel::message::ErrorCode) -> CliError {
    use keep_sync::channel::message::ErrorCode as E;
    let message = match code {
        E::Busy => "the other device is pairing or syncing with another device; try again shortly",
        E::CodeWrong => "the code is not correct",
        E::CodeLocked => "too many wrong codes; the window closed, open a new one",
        E::NotInGroup => "the other device is not in this sync group",
        E::Removed => "this device or the other was removed from the sync group",
        E::AlreadyMember => "this device is already in that sync group",
        E::OtherGroup => {
            "this device is in another sync group; leave it first ('otpkeep group leave')"
        }
        E::GroupFull => "the sync group already has 16 devices",
        E::VersionMismatch => {
            "the two devices speak different protocol versions; update the older one"
        }
        E::PeerClock => "the other device's clock is more than a day ahead; set its time",
        E::TooLarge => "the data is larger than one sync allows",
        E::DigestMismatch => "the two devices ended with different data; please sync again",
        E::Rejected => "the other device declined",
        E::SasMismatch => "the check codes did not match; pairing was cancelled",
        E::Cancelled => "the session was cancelled or timed out; nothing was changed",
        E::ReadOnly => "the data is read-only",
        E::Protocol => "the other device sent something unexpected",
    };
    not_synced(code.as_str(), message)
}

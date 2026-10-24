//! Pairing, sync and the devices of the sync group from the command line (design/06-sync.md).
//!
//! The sessions run on a Tokio runtime; questions to the person (allow, check code, codes to type) are asked
//! on the calling thread while the session keeps running, so keepalives and timers continue meanwhile.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keep_sync::lan::announce::{Announcement, FoundWindow};
use keep_sync::lan::interfaces::Eligible;
use keep_sync::lan::window::WindowKind;
use keep_sync::transport::{Acceptor, Route, Via};

use crate::args::Transport;
use keep_vault::Platform;

use crate::{CliError, EXIT_INVALID_INPUT, EXIT_NOT_SYNCED};

mod devices;
mod pair;
mod sync;

pub(crate) use devices::{devices, leave, remove_device, rename};
pub(crate) use pair::{JoinFlags, pair_join, pair_open};
pub(crate) use sync::{sync_join, sync_wait};

/// `JOIN_BROWSE`: how long to search for windows.
const BROWSE: Duration = Duration::from_secs(30);
/// After the first window appears, how long to keep collecting others.
const BROWSE_SETTLE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(50);

/// Where sessions run: the real local network, or loopback with an in-process directory of windows (tests).
#[derive(Clone)]
pub enum Network {
    /// Eligible cards and mDNS.
    System,
    /// 127.0.0.1 only; windows are announced in this directory.
    Loopback(WindowDirectory),
}

/// The loopback stand-in for discovery, shared by the devices of one test.
#[derive(Clone, Default)]
pub struct WindowDirectory(Arc<Mutex<Vec<FoundWindow>>>);

/// Keeps a window announced while alive.
pub(crate) enum Announced {
    /// Held only to keep announcing until dropped.
    System {
        _advertiser: keep_sync::lan::mdns::Advertiser,
    },
    Loopback(WindowDirectory, [u8; 16]),
}

impl Drop for Announced {
    fn drop(&mut self) {
        if let Self::Loopback(directory, random) = self
            && let Ok(mut windows) = directory.0.lock()
        {
            windows.retain(|w| w.announcement.window_random != *random);
        }
    }
}

/// A window's transports, ready to take connections.
pub(crate) struct Opened {
    /// Every transport's connections.
    pub(crate) acceptor: Acceptor,
    /// The local network's addresses and port, when it is used.
    pub(crate) lan: Option<(Vec<Eligible>, u16)>,
    /// Transports that could not be used, for the person.
    pub(crate) notes: Vec<String>,
}

impl Network {
    /// Opens a window on the chosen transports (both when `via` is `Any`): listening and announcing on the local
    /// network, advertising over Bluetooth. A transport that cannot be used is noted, unless it was the one asked
    /// for; at least one must work.
    pub(crate) async fn open(
        &self,
        via: Transport,
        announcement: &Announcement,
    ) -> Result<Opened, CliError> {
        let mut opened = Opened {
            acceptor: Acceptor::new(),
            lan: None,
            notes: Vec::new(),
        };
        if via != Transport::Bluetooth {
            match self.open_lan(&mut opened.acceptor, announcement).await {
                Ok(lan) => opened.lan = Some(lan),
                Err(error) if via == Transport::Any => opened
                    .notes
                    .push(format!("Local network not used: {}", error.message)),
                Err(error) => return Err(error),
            }
        }
        if via != Transport::Lan && matches!(self, Self::System) {
            match keep_ble::BleWindow::open(announcement, opened.acceptor.sender()).await {
                Ok(window) => opened.acceptor.hold(window),
                Err(error) if via == Transport::Any => {
                    opened.notes.push(format!("Bluetooth not used: {error}"));
                }
                Err(error) => return Err(not_synced(error.code(), &error.to_string())),
            }
        } else if via == Transport::Bluetooth {
            return Err(not_synced(
                "BLUETOOTH_UNSUPPORTED",
                "Bluetooth is not available here",
            ));
        }
        if opened.lan.is_none() && via == Transport::Any && opened.notes.len() == 2 {
            return Err(not_synced(
                "NO_ELIGIBLE_INTERFACE",
                &opened.notes.join("; "),
            ));
        }
        Ok(opened)
    }

    async fn open_lan(
        &self,
        acceptor: &mut Acceptor,
        announcement: &Announcement,
    ) -> Result<(Vec<Eligible>, u16), CliError> {
        let addrs = self.addresses()?;
        let listeners = keep_sync::flow::listen::Listeners::bind(&addrs)
            .await
            .map_err(|_| {
                not_synced(
                    "NO_ELIGIBLE_INTERFACE",
                    "the window could not start listening",
                )
            })?;
        let port = listeners.port();
        let announced = self.announce(&addrs, port, announcement)?;
        acceptor.add_lan(listeners);
        acceptor.hold(announced);
        Ok((addrs, port))
    }

    /// The addresses to listen on.
    pub(crate) fn addresses(&self) -> Result<Vec<Eligible>, CliError> {
        let addrs = match self {
            Self::System => {
                keep_sync::lan::interfaces::eligible(&keep_sync::lan::system::system_interfaces())
            }
            Self::Loopback(_) => vec![Eligible {
                interface: "lo0".to_owned(),
                index: 1,
                addr: std::net::IpAddr::from([127, 0, 0, 1]),
                prefix: 8,
            }],
        };
        if addrs.is_empty() {
            return Err(not_synced(
                "NO_ELIGIBLE_INTERFACE",
                "there is no usable local network connection (VPN and cellular are not used)",
            ));
        }
        Ok(addrs)
    }

    /// Announces a window listening on `port`.
    pub(crate) fn announce(
        &self,
        addrs: &[Eligible],
        port: u16,
        announcement: &Announcement,
    ) -> Result<Announced, CliError> {
        match self {
            Self::System => keep_sync::lan::mdns::Advertiser::start(addrs, port, announcement)
                .map(|advertiser| Announced::System {
                    _advertiser: advertiser,
                })
                .map_err(|error| not_synced("NO_ELIGIBLE_INTERFACE", &error.to_string())),
            Self::Loopback(directory) => {
                let found = FoundWindow {
                    announcement: announcement.clone(),
                    route: Route::Lan {
                        addrs: addrs.iter().map(|a| (a.addr, None)).collect(),
                        port,
                    },
                };
                if let Ok(mut windows) = directory.0.lock() {
                    windows.push(found);
                }
                Ok(Announced::Loopback(
                    directory.clone(),
                    announcement.window_random,
                ))
            }
        }
    }

    /// Windows of `kind` that `wanted` accepts, found on the chosen transports within [`BROWSE`].
    pub(crate) async fn browse(
        &self,
        via: Transport,
        kind: WindowKind,
        wanted: impl Fn(&Announcement) -> bool + Copy,
    ) -> Result<Vec<FoundWindow>, CliError> {
        let bluetooth = async {
            if via == Transport::Lan || !matches!(self, Self::System) {
                return Ok(Vec::new());
            }
            keep_ble::browse(BROWSE, BROWSE_SETTLE).await
        };
        let lan = async {
            if via == Transport::Bluetooth {
                return Ok(Vec::new());
            }
            self.browse_lan(kind, wanted).await
        };
        let (lan, bluetooth) = tokio::join!(lan, bluetooth);
        let bluetooth = match (bluetooth, via) {
            (Ok(found), _) => found,
            (Err(error), Transport::Bluetooth) => {
                return Err(not_synced(error.code(), &error.to_string()));
            }
            (Err(_), _) => Vec::new(),
        };
        let lan = match (lan, via) {
            (Ok(found), _) => found,
            (Err(error), Transport::Lan) => return Err(error),
            (Err(_), _) => Vec::new(),
        };
        let accepted = bluetooth
            .into_iter()
            .filter(|w| w.announcement.kind == kind && wanted(&w.announcement));
        Ok(keep_sync::lan::announce::merge_found(
            lan.into_iter().chain(accepted).collect(),
        ))
    }

    async fn browse_lan(
        &self,
        kind: WindowKind,
        wanted: impl Fn(&Announcement) -> bool,
    ) -> Result<Vec<FoundWindow>, CliError> {
        let accept = |w: &FoundWindow| w.announcement.kind == kind && wanted(&w.announcement);
        let mut found: Vec<FoundWindow> = Vec::new();
        let deadline = tokio::time::Instant::now() + BROWSE;
        let mut settle: Option<tokio::time::Instant> = None;
        let browser = match self {
            Self::System => {
                let addrs = self.addresses()?;
                Some(
                    keep_sync::lan::mdns::Browser::start(&addrs)
                        .map_err(|error| not_synced("NO_ELIGIBLE_INTERFACE", &error.to_string()))?,
                )
            }
            Self::Loopback(_) => None,
        };
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline || settle.is_some_and(|s| now >= s) {
                break;
            }
            let next = match (&browser, self) {
                (Some(browser), _) => tokio::time::timeout(POLL, browser.next())
                    .await
                    .ok()
                    .flatten()
                    .into_iter()
                    .collect(),
                (None, Self::Loopback(directory)) => {
                    tokio::time::sleep(POLL).await;
                    directory.0.lock().map(|w| w.clone()).unwrap_or_default()
                }
                (None, Self::System) => Vec::new(),
            };
            for window in next.into_iter().filter(|w| accept(w)) {
                if !found.iter().any(|f| {
                    f.announcement.window_random == window.announcement.window_random
                        && f.route.via() == window.route.via()
                }) {
                    found.push(window);
                    settle.get_or_insert(tokio::time::Instant::now() + BROWSE_SETTLE);
                }
            }
        }
        Ok(found)
    }
}

/// This device's platform, as other devices will show it.
pub(crate) fn this_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Linux
    }
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

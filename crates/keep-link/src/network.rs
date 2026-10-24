//! Windows on the transports: listening and announcing, and searching (design/06-sync.md §3, ADR-017).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keep_sync::lan::announce::{Announcement, FoundWindow};
use keep_sync::lan::interfaces::Eligible;
use keep_sync::lan::window::WindowKind;
use keep_sync::transport::{Acceptor, Route, Via};

/// `JOIN_BROWSE`: how long to search for windows.
pub const BROWSE: Duration = Duration::from_secs(30);
/// After the first window appears, how long to keep collecting others.
pub const BROWSE_SETTLE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(50);

/// Which transports a pairing or sync may use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Transport {
    /// Both the local network and Bluetooth.
    #[default]
    Any,
    /// The local network only.
    Lan,
    /// Bluetooth only.
    Bluetooth,
}

/// Why a window could not be opened or searched for. The code is stable (design/10-reference.md §4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct LinkError {
    /// For example `NO_ELIGIBLE_INTERFACE` or `BLUETOOTH_OFF`.
    pub code: &'static str,
    /// In English, for logs and the command line.
    pub message: String,
}

impl LinkError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A transport the window does without, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The transport.
    pub via: Via,
    /// Why it could not be used.
    pub error: LinkError,
}

/// Where sessions run: the real local network and Bluetooth, or loopback with an in-process directory of
/// windows (tests).
#[derive(Clone)]
pub enum Network {
    /// Eligible network cards, mDNS and Bluetooth.
    System,
    /// 127.0.0.1 only; windows are announced in this directory.
    Loopback(WindowDirectory),
}

/// The loopback stand-in for discovery, shared by the devices of one test.
#[derive(Clone, Default)]
pub struct WindowDirectory(Arc<Mutex<Vec<FoundWindow>>>);

/// Keeps a window announced while alive.
enum Announced {
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
pub struct Opened {
    /// Every transport's connections.
    pub acceptor: Acceptor,
    /// The local network's addresses and port, when it is used.
    pub lan: Option<(Vec<Eligible>, u16)>,
    /// Transports that could not be used.
    pub skipped: Vec<Skipped>,
}

impl Network {
    /// Opens a window on the chosen transports (both when `via` is `Any`): listening and announcing on the
    /// local network, advertising over Bluetooth. A transport that cannot be used is skipped, unless it was the
    /// one asked for; at least one must work.
    ///
    /// # Errors
    ///
    /// Returns [`LinkError`] when no transport could be opened.
    pub async fn open(
        &self,
        via: Transport,
        announcement: &Announcement,
    ) -> Result<Opened, LinkError> {
        let mut opened = Opened {
            acceptor: Acceptor::new(),
            lan: None,
            skipped: Vec::new(),
        };
        if via != Transport::Bluetooth {
            match self.open_lan(&mut opened.acceptor, announcement).await {
                Ok(lan) => opened.lan = Some(lan),
                Err(error) if via == Transport::Any => opened.skipped.push(Skipped {
                    via: Via::Lan,
                    error,
                }),
                Err(error) => return Err(error),
            }
        }
        if via != Transport::Lan && matches!(self, Self::System) {
            match keep_ble::BleWindow::open(announcement, opened.acceptor.sender()).await {
                Ok(window) => opened.acceptor.hold(window),
                Err(error) => {
                    let error = LinkError::new(error.code(), error.to_string());
                    if via == Transport::Any {
                        opened.skipped.push(Skipped {
                            via: Via::Bluetooth,
                            error,
                        });
                    } else {
                        return Err(error);
                    }
                }
            }
        } else if via == Transport::Bluetooth {
            return Err(LinkError::new(
                "BLUETOOTH_UNSUPPORTED",
                "Bluetooth is not available here",
            ));
        }
        if opened.lan.is_none() && via == Transport::Any && opened.skipped.len() == 2 {
            let reasons: Vec<String> = opened
                .skipped
                .iter()
                .map(|s| s.error.message.clone())
                .collect();
            return Err(LinkError::new("NO_ELIGIBLE_INTERFACE", reasons.join("; ")));
        }
        Ok(opened)
    }

    async fn open_lan(
        &self,
        acceptor: &mut Acceptor,
        announcement: &Announcement,
    ) -> Result<(Vec<Eligible>, u16), LinkError> {
        let addrs = self.addresses()?;
        let listeners = keep_sync::flow::listen::Listeners::bind(&addrs)
            .await
            .map_err(|_| {
                LinkError::new(
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
    ///
    /// # Errors
    ///
    /// Returns `NO_ELIGIBLE_INTERFACE` when there is no usable local network.
    pub fn addresses(&self) -> Result<Vec<Eligible>, LinkError> {
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
            return Err(LinkError::new(
                "NO_ELIGIBLE_INTERFACE",
                "there is no usable local network connection (VPN and cellular are not used)",
            ));
        }
        Ok(addrs)
    }

    /// Announces a window listening on `port`.
    fn announce(
        &self,
        addrs: &[Eligible],
        port: u16,
        announcement: &Announcement,
    ) -> Result<Announced, LinkError> {
        match self {
            Self::System => keep_sync::lan::mdns::Advertiser::start(addrs, port, announcement)
                .map(|advertiser| Announced::System {
                    _advertiser: advertiser,
                })
                .map_err(|error| LinkError::new("NO_ELIGIBLE_INTERFACE", error.to_string())),
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
    ///
    /// # Errors
    ///
    /// Returns [`LinkError`] when the one transport asked for cannot search.
    pub async fn browse(
        &self,
        via: Transport,
        kind: WindowKind,
        wanted: impl Fn(&Announcement) -> bool + Copy,
    ) -> Result<Vec<FoundWindow>, LinkError> {
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
                return Err(LinkError::new(error.code(), error.to_string()));
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
    ) -> Result<Vec<FoundWindow>, LinkError> {
        let accept = |w: &FoundWindow| w.announcement.kind == kind && wanted(&w.announcement);
        let mut found: Vec<FoundWindow> = Vec::new();
        let deadline = tokio::time::Instant::now() + BROWSE;
        let mut settle: Option<tokio::time::Instant> = None;
        let browser = match self {
            Self::System => {
                let addrs = self.addresses()?;
                Some(
                    keep_sync::lan::mdns::Browser::start(&addrs).map_err(|error| {
                        LinkError::new("NO_ELIGIBLE_INTERFACE", error.to_string())
                    })?,
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

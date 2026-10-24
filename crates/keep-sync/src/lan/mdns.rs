//! Announcing and finding windows with mDNS on the desktop (design/07-platform.md §5).
//!
//! Only the cards chosen by [`crate::lan::interfaces::eligible`] send and receive. The instance and host names
//! are random, so the computer's own name is never announced; the announcement itself carries only the
//! window's record. A found window is kept only with addresses on the link of one of this device's cards.

use std::fmt::Write as _;
use std::net::IpAddr;

use mdns_sd::{IfKind, ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo};

use super::announce::{Announcement, FoundWindow, SERVICE_TYPE, from_txt, to_txt};
use super::interfaces::{Eligible, peer_is_on_link};

/// Why discovery could not run.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// No card can be used (`NO_ELIGIBLE_INTERFACE`).
    #[error("there is no usable local network connection")]
    NoInterface,
    /// The mDNS service failed.
    #[error("local network discovery failed")]
    Mdns(#[from] mdns_sd::Error),
    /// No random bytes for the names.
    #[error("the system random number generator is unavailable")]
    Random,
}

/// An mDNS daemon limited to the eligible cards.
fn daemon(eligible: &[Eligible]) -> Result<ServiceDaemon, DiscoveryError> {
    if eligible.is_empty() {
        return Err(DiscoveryError::NoInterface);
    }
    let daemon = ServiceDaemon::new()?;
    daemon.disable_interface(IfKind::All)?;
    let mut names: Vec<String> = eligible.iter().map(|e| e.interface.clone()).collect();
    names.dedup();
    for name in names {
        daemon.enable_interface(IfKind::Name(name))?;
    }
    Ok(daemon)
}

fn random_label() -> Result<String, DiscoveryError> {
    let mut bytes = [0u8; 6];
    getrandom::fill(&mut bytes).map_err(|_| DiscoveryError::Random)?;
    Ok(bytes.iter().fold(String::from("otpkeep-"), |mut name, b| {
        // Writing to a String cannot fail.
        let _ = write!(name, "{b:02x}");
        name
    }))
}

/// An open window's announcement; it stops when dropped.
pub struct Advertiser {
    daemon: ServiceDaemon,
}

impl Advertiser {
    /// Announces `announcement` for a window listening on `port` at the eligible addresses.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`].
    pub fn start(
        eligible: &[Eligible],
        port: u16,
        announcement: &Announcement,
    ) -> Result<Self, DiscoveryError> {
        let daemon = daemon(eligible)?;
        let label = random_label()?;
        let addrs: Vec<IpAddr> = eligible.iter().map(|e| e.addr).collect();
        let txt = to_txt(announcement);
        let properties: Vec<(&str, &str)> =
            txt.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let host = format!("{label}.local.");
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &label,
            &host,
            &addrs[..],
            port,
            &properties[..],
        )?;
        daemon.register(info)?;
        Ok(Self { daemon })
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

/// Searching for windows; stops when dropped.
pub struct Browser {
    daemon: ServiceDaemon,
    events: mdns_sd::Receiver<ServiceEvent>,
    eligible: Vec<Eligible>,
}

impl Browser {
    /// Starts searching on the eligible cards.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`].
    pub fn start(eligible: &[Eligible]) -> Result<Self, DiscoveryError> {
        let daemon = daemon(eligible)?;
        let events = daemon.browse(SERVICE_TYPE)?;
        Ok(Self {
            daemon,
            events,
            eligible: eligible.to_vec(),
        })
    }

    /// The next window found, skipping records that are malformed or not on this device's links. `None` when
    /// the search ended.
    pub async fn next(&self) -> Option<FoundWindow> {
        loop {
            match self.events.recv_async().await.ok()? {
                ServiceEvent::ServiceResolved(service) => {
                    if let Some(found) = self.found(&service) {
                        return Some(found);
                    }
                }
                ServiceEvent::SearchStopped(_) => return None,
                _ => {}
            }
        }
    }

    fn found(&self, service: &ResolvedService) -> Option<FoundWindow> {
        let txt: Vec<(String, String)> = service
            .txt_properties
            .iter()
            .map(|p| (p.key().to_owned(), p.val_str().to_owned()))
            .collect();
        let announcement = from_txt(&txt).ok()?;
        let mut addrs: Vec<(IpAddr, Option<u32>)> = service
            .addresses
            .iter()
            .map(|scoped| {
                let scope = match scoped {
                    ScopedIp::V6(v6) => Some(v6.scope_id().index),
                    _ => None,
                };
                (scoped.to_ip_addr(), scope)
            })
            .filter(|(addr, scope)| {
                self.eligible
                    .iter()
                    .any(|e| peer_is_on_link(e, *addr, *scope))
            })
            .collect();
        addrs.sort();
        (!addrs.is_empty()).then_some(FoundWindow {
            announcement,
            route: crate::transport::Route::Lan {
                addrs,
                port: service.port,
            },
        })
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Advertiser, Browser};
    use crate::lan::announce::Announcement;
    use crate::lan::interfaces::eligible;
    use crate::lan::system::system_interfaces;
    use crate::lan::window::WindowKind;

    /// Needs a real local network; run by hand with `--ignored`.
    #[tokio::test]
    #[ignore = "needs a local network with multicast"]
    async fn a_window_announced_here_is_found_here() {
        let cards = eligible(&system_interfaces());
        let announcement = Announcement {
            kind: WindowKind::Pair,
            lan_name: "OTPKeep-TEST".to_owned(),
            short_fingerprint: "ABCD".to_owned(),
            remaining_s: 60,
            window_random: [7; 16],
            group_tag: None,
        };
        let advertiser = Advertiser::start(&cards, 45_123, &announcement).unwrap();
        let browser = Browser::start(&cards).unwrap();

        let found = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let found = browser.next().await.unwrap();
                if found.announcement == announcement {
                    return found;
                }
            }
        })
        .await
        .unwrap();

        assert!(matches!(
            found.route,
            crate::transport::Route::Lan { port: 45_123, .. }
        ));
        drop((advertiser, browser));
    }
}

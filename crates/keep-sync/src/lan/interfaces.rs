//! Which network cards and addresses sync may use, and whether a peer is on the same link
//! (design/06-sync.md §3.4).
//!
//! Only physical wired or wireless cards with private addresses count: IPv4 RFC 1918, IPv6 link-local
//! (`fe80::/10`) and unique local (`fc00::/7`). Tunnels, VPNs, bridges and virtual cards are excluded by kind,
//! flags and name. Nothing ever listens on an unspecified address. The checks are pure; the platform layer
//! only describes its cards.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Name prefixes of tunnels, VPNs, bridges and virtual cards, matched without regard to case.
const EXCLUDED_NAME_PREFIXES: &[&str] = &[
    "utun",
    "tun",
    "tap",
    "wg",
    "docker",
    "vethernet",
    "ppp",
    "bridge",
    "virbr",
    "vmnet",
    "vboxnet",
    "awdl",
    "llw",
    "br-",
    "veth",
    "zt",
    "tailscale",
    "ipsec",
    "gif",
    "stf",
    "anpi",
    "ap1",
    "lo",
];

/// What kind of card the system reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceKind {
    /// Wired Ethernet.
    Ethernet,
    /// Wi-Fi.
    Wireless,
    /// Anything else: cellular, tunnels, virtual cards.
    Other,
}

/// A network card as the platform describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the flags the system reports"
)]
pub struct Interface {
    /// System name, such as `en0`.
    pub name: String,
    /// System index, the scope of IPv6 link-local addresses.
    pub index: u32,
    /// Card kind.
    pub kind: InterfaceKind,
    /// The card is up.
    pub is_up: bool,
    /// Loopback.
    pub is_loopback: bool,
    /// Point-to-point link, as tunnels and VPNs are.
    pub is_point_to_point: bool,
    /// Multicast works, which discovery needs.
    pub supports_multicast: bool,
    /// Addresses with their prefix lengths.
    pub addrs: Vec<(IpAddr, u8)>,
}

/// An address sync may listen on, with what the peer check needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eligible {
    /// The card's name.
    pub interface: String,
    /// The card's index.
    pub index: u32,
    /// The address.
    pub addr: IpAddr,
    /// Its prefix length.
    pub prefix: u8,
}

/// The addresses sync may use, in the order the cards and addresses were given.
#[must_use]
pub fn eligible(cards: &[Interface]) -> Vec<Eligible> {
    cards
        .iter()
        .filter(|card| card_is_usable(card))
        .flat_map(|card| {
            card.addrs
                .iter()
                .filter(|(addr, _)| is_private(*addr))
                .map(|(addr, prefix)| Eligible {
                    interface: card.name.clone(),
                    index: card.index,
                    addr: *addr,
                    prefix: *prefix,
                })
        })
        .collect()
}

fn card_is_usable(card: &Interface) -> bool {
    let name = card.name.to_lowercase();
    matches!(card.kind, InterfaceKind::Ethernet | InterfaceKind::Wireless)
        && card.is_up
        && !card.is_loopback
        && !card.is_point_to_point
        && card.supports_multicast
        && !EXCLUDED_NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// RFC 1918 IPv4, IPv6 link-local or unique local.
#[must_use]
pub fn is_private(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(v6) => is_link_local(v6) || is_unique_local(v6),
    }
}

fn is_link_local(v6: Ipv6Addr) -> bool {
    v6.segments()[0] & 0xffc0 == 0xfe80
}

fn is_unique_local(v6: Ipv6Addr) -> bool {
    v6.segments()[0] & 0xfe00 == 0xfc00
}

/// Whether `peer` is on the link of the card that accepted it: the same IPv4 subnet, the same IPv6 prefix for
/// unique local addresses, or, for link-local addresses, the same card (`peer_scope` is the scope id the
/// connection arrived with).
#[must_use]
pub fn peer_is_on_link(local: &Eligible, peer: IpAddr, peer_scope: Option<u32>) -> bool {
    if !is_private(peer) {
        return false;
    }
    match (local.addr, peer) {
        (IpAddr::V4(ours), IpAddr::V4(theirs)) => same_prefix_v4(ours, theirs, local.prefix),
        (IpAddr::V6(ours), IpAddr::V6(theirs)) if is_link_local(ours) && is_link_local(theirs) => {
            peer_scope == Some(local.index)
        }
        (IpAddr::V6(ours), IpAddr::V6(theirs))
            if is_unique_local(ours) && is_unique_local(theirs) =>
        {
            same_prefix(&ours.octets(), &theirs.octets(), local.prefix)
        }
        _ => false,
    }
}

fn same_prefix_v4(ours: Ipv4Addr, theirs: Ipv4Addr, prefix: u8) -> bool {
    same_prefix(&ours.octets(), &theirs.octets(), prefix)
}

fn same_prefix(ours: &[u8], theirs: &[u8], prefix: u8) -> bool {
    let bits = usize::from(prefix).min(ours.len() * 8);
    let (whole, rest) = (bits / 8, bits % 8);
    if ours[..whole] != theirs[..whole] {
        return false;
    }
    if rest == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rest);
    ours[whole] & mask == theirs[whole] & mask
}

#[cfg(test)]
mod tests;

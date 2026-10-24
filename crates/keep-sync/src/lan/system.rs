//! The desktop systems' network cards, through `netdev`. Mobile platforms describe their cards through their
//! own bridge (design/07-platform.md §5).

use std::net::IpAddr;

use netdev::interface::types::InterfaceType;

use super::interfaces::{Interface, InterfaceKind};

/// The cards this system reports.
#[must_use]
pub fn system_interfaces() -> Vec<Interface> {
    netdev::get_interfaces().iter().map(describe).collect()
}

fn describe(card: &netdev::Interface) -> Interface {
    let v4 = card
        .ipv4
        .iter()
        .map(|net| (IpAddr::V4(net.addr()), net.prefix_len()));
    let v6 = card
        .ipv6
        .iter()
        .map(|net| (IpAddr::V6(net.addr()), net.prefix_len()));
    Interface {
        name: card.name.clone(),
        index: card.index,
        kind: kind(card.if_type),
        is_up: card.is_up(),
        is_loopback: card.is_loopback(),
        is_point_to_point: card.is_point_to_point(),
        supports_multicast: card.is_multicast(),
        addrs: v4.chain(v6).collect(),
    }
}

fn kind(if_type: InterfaceType) -> InterfaceKind {
    match if_type {
        InterfaceType::Ethernet
        | InterfaceType::Ethernet3Megabit
        | InterfaceType::FastEthernetT
        | InterfaceType::FastEthernetFx
        | InterfaceType::GigabitEthernet => InterfaceKind::Ethernet,
        InterfaceType::Wireless80211 => InterfaceKind::Wireless,
        _ => InterfaceKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::system_interfaces;
    use crate::lan::interfaces::eligible;

    #[test]
    fn this_machine_never_offers_loopback_or_public_addresses() {
        for found in eligible(&system_interfaces()) {
            assert!(
                !found.addr.is_loopback() && !found.addr.is_unspecified(),
                "{found:?}"
            );
            assert!(crate::lan::interfaces::is_private(found.addr));
        }
    }
}

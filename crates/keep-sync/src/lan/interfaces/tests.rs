use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{Eligible, Interface, InterfaceKind, eligible, peer_is_on_link};

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn nic(name: &str, kind: InterfaceKind, addrs: &[(&str, u8)]) -> Interface {
    Interface {
        name: name.to_owned(),
        index: 4,
        kind,
        is_up: true,
        is_loopback: false,
        is_point_to_point: false,
        supports_multicast: true,
        addrs: addrs.iter().map(|(a, p)| (ip(a), *p)).collect(),
    }
}

fn addrs(list: &[Eligible]) -> Vec<IpAddr> {
    list.iter().map(|e| e.addr).collect()
}

#[test]
fn private_addresses_on_wired_and_wireless_cards_are_used() {
    let cards = [
        nic(
            "en0",
            InterfaceKind::Wireless,
            &[("192.168.1.20", 24), ("fe80::1", 64), ("fd12::5", 64)],
        ),
        nic(
            "eth0",
            InterfaceKind::Ethernet,
            &[("10.0.0.7", 8), ("172.16.3.4", 12)],
        ),
    ];

    assert_eq!(
        addrs(&eligible(&cards)),
        vec![
            ip("192.168.1.20"),
            ip("fe80::1"),
            ip("fd12::5"),
            ip("10.0.0.7"),
            ip("172.16.3.4")
        ]
    );
}

#[test]
fn public_and_special_addresses_are_never_used() {
    let cards = [nic(
        "en0",
        InterfaceKind::Wireless,
        &[
            ("8.8.8.8", 24),
            ("172.32.0.1", 16),
            ("100.64.0.1", 10),
            ("169.254.3.3", 16),
            ("2001:db8::1", 64),
            ("::1", 128),
            ("0.0.0.0", 0),
        ],
    )];

    assert!(eligible(&cards).is_empty());
}

#[test]
fn tunnels_vpns_bridges_and_virtual_cards_are_excluded() {
    let names = [
        "utun3",
        "tun0",
        "tap1",
        "wg0",
        "docker0",
        "vEthernet (WSL)",
        "ppp0",
        "bridge100",
        "virbr0",
        "vmnet8",
        "vboxnet0",
        "awdl0",
        "llw0",
        "br-1a2b",
        "veth12ab",
        "zt0",
        "tailscale0",
        "ipsec0",
    ];
    for name in names {
        let card = nic(name, InterfaceKind::Ethernet, &[("192.168.5.5", 24)]);
        assert!(eligible(&[card]).is_empty(), "{name} must not be used");
    }
    let other_kind = nic("en5", InterfaceKind::Other, &[("192.168.5.5", 24)]);
    assert!(
        eligible(&[other_kind]).is_empty(),
        "only wired and wireless cards"
    );
}

#[test]
fn cards_that_are_down_loopback_point_to_point_or_without_multicast_are_excluded() {
    let base = nic("en0", InterfaceKind::Ethernet, &[("192.168.1.2", 24)]);
    let variants = [
        Interface {
            is_up: false,
            ..base.clone()
        },
        Interface {
            is_loopback: true,
            ..base.clone()
        },
        Interface {
            is_point_to_point: true,
            ..base.clone()
        },
        Interface {
            supports_multicast: false,
            ..base.clone()
        },
    ];

    for card in variants {
        assert!(eligible(std::slice::from_ref(&card)).is_empty(), "{card:?}");
    }
    assert_eq!(eligible(&[base]).len(), 1);
}

#[test]
fn a_peer_must_be_on_the_subnet_of_the_accepting_card() {
    let cards = eligible(&[nic(
        "en0",
        InterfaceKind::Wireless,
        &[("192.168.1.20", 24), ("fe80::1", 64)],
    )]);
    let v4 = cards.iter().find(|e| e.addr.is_ipv4()).unwrap();
    let v6 = cards.iter().find(|e| e.addr.is_ipv6()).unwrap();

    assert!(peer_is_on_link(v4, ip("192.168.1.77"), None));
    assert!(!peer_is_on_link(v4, ip("192.168.2.77"), None));
    assert!(!peer_is_on_link(v4, ip("8.8.8.8"), None));
    assert!(!peer_is_on_link(
        v4,
        IpAddr::V6(Ipv4Addr::new(192, 168, 1, 77).to_ipv6_mapped()),
        None
    ));
    assert!(
        peer_is_on_link(v6, ip("fe80::99"), Some(4)),
        "link-local on the same card"
    );
    assert!(
        !peer_is_on_link(v6, ip("fe80::99"), Some(5)),
        "link-local on another card"
    );
    assert!(!peer_is_on_link(v6, ip("fe80::99"), None));
    assert!(!peer_is_on_link(
        v6,
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        Some(4)
    ));
}

#[test]
fn unique_local_peers_must_share_the_prefix() {
    let cards = eligible(&[nic("en0", InterfaceKind::Ethernet, &[("fd12:3456::1", 64)])]);

    assert!(peer_is_on_link(&cards[0], ip("fd12:3456::abcd"), None));
    assert!(!peer_is_on_link(&cards[0], ip("fd12:3457::abcd"), None));
}

#[test]
fn loopback_is_never_offered_and_only_ever_matches_loopback() {
    let local = Eligible {
        interface: "lo0".to_owned(),
        index: 1,
        addr: ip("127.0.0.1"),
        prefix: 8,
    };

    assert!(eligible(&[nic("lo0", InterfaceKind::Ethernet, &[("127.0.0.1", 8)])]).is_empty());
    assert!(peer_is_on_link(&local, ip("127.0.0.1"), None));
    assert!(!peer_is_on_link(&local, ip("192.168.1.2"), None));
}

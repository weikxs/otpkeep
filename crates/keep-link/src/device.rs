//! This device as other devices see it: its platform, its introduction in a session, its name on the local
//! network, and pairing invites (design/06-sync.md §2, §4.1).

use keep_sync::channel::PROTOCOL_VERSION;
use keep_sync::channel::code::Secret;
use keep_sync::channel::hello::{Hello, is_valid_lan_name};
use keep_sync::flow::pair_join::PairTarget;
use keep_sync::identity::{DeviceIdentity, short_fingerprint};
use keep_sync::invite::{PairInvite, new_random};
use keep_sync::lan::announce::Announcement;
use keep_sync::lan::interfaces::{Eligible, peer_is_on_link};
use keep_sync::membership::membership;
use keep_sync::transport::Route;
use keep_vault::{Platform, VaultDoc};
use zeroize::Zeroizing;

use crate::LinkError;

/// This device's platform, as other devices will show it.
#[must_use]
pub const fn this_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "ios") {
        Platform::Ios
    } else if cfg!(target_os = "android") {
        Platform::Android
    } else {
        Platform::Linux
    }
}

/// This device's introduction in a session.
#[must_use]
pub fn hello(identity: &DeviceIdentity, doc: &VaultDoc) -> Hello {
    Hello {
        device: identity.device_id(),
        dx_cert: Some(identity.dx_cert()),
        platform: this_platform(),
        lan_name: own_name(doc, identity),
        versions: vec![PROTOCOL_VERSION],
    }
}

/// This device's local network name in its group, or `OTPKeep-` and its short fingerprint.
#[must_use]
pub fn own_name(doc: &VaultDoc, identity: &DeviceIdentity) -> String {
    doc.group
        .as_ref()
        .and_then(|g| {
            membership(g)
                .members
                .get(&identity.device_id())
                .and_then(|m| m.lan_name.clone())
        })
        .unwrap_or_else(|| format!("OTPKeep-{}", short_fingerprint(identity.device_id())))
}

/// The name given, checked, or `OTPKeep-` and four random hex digits.
///
/// # Errors
///
/// Returns `INVALID_NAME` for a name that is empty, too long or has surrounding spaces, `RANDOM` when no
/// random digits can be made.
pub fn lan_name(name: Option<&str>) -> Result<String, LinkError> {
    match name {
        Some(name) if is_valid_lan_name(name) => Ok(name.to_owned()),
        Some(_) => Err(LinkError {
            code: "INVALID_NAME",
            message: "the name must have 1 to 32 characters without surrounding spaces".to_owned(),
        }),
        None => {
            let random = new_random().map_err(|_| random_failed())?;
            Ok(format!("OTPKeep-{:02X}{:02X}", random[0], random[1]))
        }
    }
}

/// The pairing invite for a window on the local network: the text in its QR code. It pairs without the code,
/// so it is shown only to the person.
#[must_use]
pub fn invite_text(
    addrs: &[Eligible],
    port: u16,
    announcement: &Announcement,
    secret: &[u8; 16],
) -> Zeroizing<String> {
    PairInvite {
        addrs: addrs
            .iter()
            .map(|a| a.addr)
            .filter(std::net::IpAddr::is_ipv4)
            .collect(),
        port,
        short_fingerprint: announcement.short_fingerprint.clone(),
        window_random: announcement.window_random,
        secret: *secret,
    }
    .encode()
}

/// The window an invite points to, kept to addresses on this device's links.
///
/// # Errors
///
/// Returns `INVALID_INVITE` for text that is not an invite, `NOT_SAME_NETWORK` when it points elsewhere.
pub fn target_from_invite(text: &str, addrs: &[Eligible]) -> Result<PairTarget, LinkError> {
    let invite = PairInvite::parse(text).map_err(|error| LinkError {
        code: "INVALID_INVITE",
        message: error.to_string(),
    })?;
    let reachable: Vec<_> = invite
        .addrs
        .iter()
        .filter(|addr| {
            addrs
                .iter()
                .any(|local| peer_is_on_link(local, **addr, None))
        })
        .map(|addr| (*addr, None))
        .collect();
    if reachable.is_empty() {
        return Err(LinkError {
            code: "NOT_SAME_NETWORK",
            message: "the pairing code points to another network".to_owned(),
        });
    }
    Ok(PairTarget {
        route: Route::Lan {
            addrs: reachable,
            port: invite.port,
        },
        window_random: invite.window_random,
        short_fingerprint: invite.short_fingerprint,
        secret: Secret::PairSecret(invite.secret),
    })
}

/// The window found on the network, with the pairing code the person typed (digits only are kept).
#[must_use]
pub fn target_with_code(window: &keep_sync::lan::announce::FoundWindow, typed: &str) -> PairTarget {
    PairTarget {
        route: window.route.clone(),
        window_random: window.announcement.window_random,
        short_fingerprint: window.announcement.short_fingerprint.clone(),
        secret: Secret::PairCode(Zeroizing::new(
            typed.chars().filter(char::is_ascii_digit).collect(),
        )),
    }
}

/// The error for a failed random source.
#[must_use]
pub fn random_failed() -> LinkError {
    LinkError {
        code: "RANDOM",
        message: "the system random number generator is unavailable".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{lan_name, target_from_invite};
    use keep_sync::lan::interfaces::Eligible;

    #[test]
    fn a_name_is_checked_or_made_up() {
        assert_eq!(lan_name(Some("办公室 Mac")).unwrap(), "办公室 Mac");
        assert_eq!(lan_name(Some(" x")).unwrap_err().code, "INVALID_NAME");
        let made = lan_name(None).unwrap();
        assert!(made.starts_with("OTPKeep-") && made.len() == 12, "{made}");
    }

    #[test]
    fn an_invite_must_be_an_invite() {
        let local = [Eligible {
            interface: "lo0".to_owned(),
            index: 1,
            addr: std::net::IpAddr::from([127, 0, 0, 1]),
            prefix: 8,
        }];

        let refused = target_from_invite("hello", &local).err().map(|e| e.code);
        assert_eq!(refused, Some("INVALID_INVITE"));
    }
}

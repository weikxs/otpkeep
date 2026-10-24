//! What an open window announces on the local network (design/06-sync.md §3.5).
//!
//! The record names the window, not the device: kind, LAN name, short fingerprint, time left and the window's
//! random value. A sync window adds a group tag that changes every hour, so members recognise their group's
//! windows and nobody else can tell whether two windows belong to the same group. No ids, codes or secrets.

use keep_passcode::base32;

use crate::lan::window::WindowKind;

/// The DNS-SD service type.
pub const SERVICE_TYPE: &str = "_otpkeep._tcp.local.";
/// The group tag changes every hour.
pub const HOUR_MS: u64 = 60 * 60 * 1000;
const PROTOCOL: &str = "1";
const TAG_CONTEXT: &[u8] = b"otpkeep tag v1";
const TAG_LEN: usize = 6;
const SHORT_FINGERPRINT_DIGITS: usize = 4;

/// What one window announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Pairing or sync.
    pub kind: WindowKind,
    /// The opener's LAN name.
    pub lan_name: String,
    /// The opener's short fingerprint.
    pub short_fingerprint: String,
    /// Seconds until the window closes.
    pub remaining_s: u32,
    /// Bound into the handshake, so a connection belongs to this window; not a secret.
    pub window_random: [u8; 16],
    /// Sync windows only: the group tag for the current hour.
    pub group_tag: Option<[u8; TAG_LEN]>,
}

/// A window someone else opened, as discovery found it.
#[derive(Debug, Clone)]
pub struct FoundWindow {
    /// Its announcement.
    pub announcement: Announcement,
    /// How to reach it.
    pub route: crate::transport::Route,
}

/// Found windows as one list: a window found on several transports (the same random value) appears once, by
/// its fastest route (the local network before Bluetooth). Order is otherwise kept.
#[must_use]
pub fn merge_found(found: Vec<FoundWindow>) -> Vec<FoundWindow> {
    let mut merged: Vec<FoundWindow> = Vec::new();
    for window in found {
        match merged
            .iter_mut()
            .find(|w| w.announcement.window_random == window.announcement.window_random)
        {
            Some(existing) if window.route.via() < existing.route.via() => *existing = window,
            Some(_) => {}
            None => merged.push(window),
        }
    }
    merged
}

/// A record that is not a window this version can join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AnnounceError {
    /// Another protocol version.
    #[error("the window uses another protocol version")]
    Version,
    /// A value is missing or malformed.
    #[error("the window's announcement is malformed")]
    Invalid,
}

/// The TXT record, as key-value pairs in a fixed order.
#[must_use]
pub fn to_txt(announcement: &Announcement) -> Vec<(String, String)> {
    let kind = match announcement.kind {
        WindowKind::Pair => "pair",
        WindowKind::Sync => "sync",
    };
    let mut txt = vec![
        ("v".to_owned(), PROTOCOL.to_owned()),
        ("w".to_owned(), kind.to_owned()),
        ("n".to_owned(), announcement.lan_name.clone()),
        ("f".to_owned(), announcement.short_fingerprint.clone()),
        ("x".to_owned(), announcement.remaining_s.to_string()),
        (
            "r".to_owned(),
            base32::encode(&announcement.window_random).to_string(),
        ),
    ];
    if let Some(tag) = announcement.group_tag {
        txt.push(("g".to_owned(), base32::encode(&tag).to_string()));
    }
    txt
}

/// Reads a TXT record.
///
/// # Errors
///
/// Returns [`AnnounceError`].
pub fn from_txt(txt: &[(String, String)]) -> Result<Announcement, AnnounceError> {
    let get = |key: &str| {
        txt.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .ok_or(AnnounceError::Invalid)
    };
    if get("v")? != PROTOCOL {
        return Err(AnnounceError::Version);
    }
    let kind = match get("w")? {
        "pair" => WindowKind::Pair,
        "sync" => WindowKind::Sync,
        _ => return Err(AnnounceError::Invalid),
    };
    let lan_name = get("n")?;
    if !crate::channel::hello::is_valid_lan_name(lan_name) {
        return Err(AnnounceError::Invalid);
    }
    let short_fingerprint = get("f")?;
    if short_fingerprint.len() != SHORT_FINGERPRINT_DIGITS
        || !short_fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
    {
        return Err(AnnounceError::Invalid);
    }
    let remaining_s = get("x")?.parse().map_err(|_| AnnounceError::Invalid)?;
    let window_random = fixed(get("r")?)?;
    let group_tag = match kind {
        WindowKind::Sync => Some(fixed(get("g")?)?),
        WindowKind::Pair => None,
    };
    Ok(Announcement {
        kind,
        lan_name: lan_name.to_owned(),
        short_fingerprint: short_fingerprint.to_owned(),
        remaining_s,
        window_random,
        group_tag,
    })
}

fn fixed<const N: usize>(text: &str) -> Result<[u8; N], AnnounceError> {
    let bytes = base32::decode(text).map_err(|_| AnnounceError::Invalid)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| AnnounceError::Invalid)
}

/// The Bluetooth "window information" value: the same record as the TXT record, one `key=value` per line, plus
/// the L2CAP channel (`p`) to connect to (design/06-sync.md §3.6). Names never contain line breaks.
#[must_use]
pub fn window_info_bytes(announcement: &Announcement, psm: u16) -> Vec<u8> {
    let mut lines: Vec<String> = to_txt(announcement)
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    lines.push(format!("p={psm}"));
    lines.join("\n").into_bytes()
}

/// Reads a Bluetooth "window information" value.
///
/// # Errors
///
/// Returns [`AnnounceError`].
pub fn from_window_info(bytes: &[u8]) -> Result<(Announcement, u16), AnnounceError> {
    let text = std::str::from_utf8(bytes).map_err(|_| AnnounceError::Invalid)?;
    let pairs: Vec<(String, String)> = text
        .split('\n')
        .map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .ok_or(AnnounceError::Invalid)
        })
        .collect::<Result<_, _>>()?;
    let psm = pairs
        .iter()
        .find(|(k, _)| k == "p")
        .and_then(|(_, v)| v.parse::<u16>().ok())
        .filter(|psm| *psm != 0)
        .ok_or(AnnounceError::Invalid)?;
    Ok((from_txt(&pairs)?, psm))
}

/// The group tag for one hour: keyed BLAKE3 of the group id and the hour number.
#[must_use]
pub fn group_tag(tag_key: &[u8; 32], group_id: &[u8; 16], hour: u64) -> [u8; TAG_LEN] {
    let mut hasher = blake3::Hasher::new_keyed(tag_key);
    hasher.update(TAG_CONTEXT);
    hasher.update(group_id);
    hasher.update(&hour.to_be_bytes());
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&hasher.finalize().as_bytes()[..TAG_LEN]);
    tag
}

/// Whether `tag` is this group's for the current hour or the hour before or after, which allows for clocks
/// that differ a little.
#[must_use]
pub fn matches_group(
    tag: &[u8; TAG_LEN],
    tag_key: &[u8; 32],
    group_id: &[u8; 16],
    now_ms: u64,
) -> bool {
    let hour = now_ms / HOUR_MS;
    [hour.saturating_sub(1), hour, hour + 1]
        .iter()
        .any(|h| group_tag(tag_key, group_id, *h) == *tag)
}

#[cfg(test)]
mod tests;

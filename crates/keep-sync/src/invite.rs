//! The pairing QR code and the codes people type (design/06-sync.md §4.1, §5.2).

use std::fmt::Write as _;
use std::net::IpAddr;

use keep_passcode::base32;
use zeroize::Zeroizing;

use crate::lan::interfaces::is_private;

const SCHEME: &str = "otpkeep-pair:";
const VERSION: &str = "1";
const SHORT_FINGERPRINT_DIGITS: usize = 4;
const MAX_ADDRS: usize = 8;

/// What the pairing QR code carries. It holds the pairing secret: showing it is a sensitive action.
#[derive(Clone, PartialEq, Eq)]
pub struct PairInvite {
    /// The opener's eligible addresses.
    pub addrs: Vec<IpAddr>,
    /// Its port.
    pub port: u16,
    /// Its short fingerprint.
    pub short_fingerprint: String,
    /// The window's random value.
    pub window_random: [u8; 16],
    /// The 128-bit pairing secret.
    pub secret: [u8; 16],
}

impl std::fmt::Debug for PairInvite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairInvite")
            .field("addrs", &self.addrs)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// Why a scanned text is not a usable invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteError {
    /// Not an OTPKeep pairing code.
    #[error("this is not an OTPKeep pairing code")]
    NotAnInvite,
    /// From a newer version.
    #[error("the pairing code is from a newer version of OTPKeep")]
    Version,
    /// An address outside the local network (`NOT_SAME_NETWORK`).
    #[error("the pairing code points outside the local network")]
    NotLocal,
    /// A value is missing or malformed.
    #[error("the pairing code is damaged")]
    Invalid,
}

impl PairInvite {
    /// The text in the QR code.
    #[must_use]
    pub fn encode(&self) -> Zeroizing<String> {
        let mut text = Zeroizing::new(format!("{SCHEME}{VERSION}?"));
        for addr in &self.addrs {
            let _ = write!(text, "h={addr}&"); // writing to a String cannot fail
        }
        let _ = write!(
            text,
            "p={}&f={}&r={}&s={}",
            self.port,
            self.short_fingerprint,
            &*base32::encode(&self.window_random),
            &*base32::encode(&self.secret)
        );
        text
    }

    /// Reads a scanned text. Every address must be private; the caller also checks it is on one of its links.
    ///
    /// # Errors
    ///
    /// Returns [`InviteError`].
    pub fn parse(text: &str) -> Result<Self, InviteError> {
        let rest = text
            .trim()
            .strip_prefix(SCHEME)
            .ok_or(InviteError::NotAnInvite)?;
        let (version, query) = rest.split_once('?').ok_or(InviteError::Invalid)?;
        if version != VERSION {
            return Err(InviteError::Version);
        }
        let pairs: Vec<(&str, &str)> = query.split('&').filter_map(|p| p.split_once('=')).collect();
        let one = |key: &str| {
            let mut found = pairs.iter().filter(|(k, _)| *k == key).map(|(_, v)| *v);
            match (found.next(), found.next()) {
                (Some(value), None) => Ok(value),
                _ => Err(InviteError::Invalid),
            }
        };
        let addrs: Vec<IpAddr> = pairs
            .iter()
            .filter(|(k, _)| *k == "h")
            .map(|(_, v)| v.parse().map_err(|_| InviteError::Invalid))
            .collect::<Result<_, _>>()?;
        if addrs.is_empty() || addrs.len() > MAX_ADDRS {
            return Err(InviteError::Invalid);
        }
        if !addrs.iter().all(|a| is_private(*a)) {
            return Err(InviteError::NotLocal);
        }
        let port: u16 = one("p")?.parse().map_err(|_| InviteError::Invalid)?;
        if port == 0 {
            return Err(InviteError::Invalid);
        }
        let short_fingerprint = one("f")?;
        if short_fingerprint.len() != SHORT_FINGERPRINT_DIGITS
            || !short_fingerprint
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        {
            return Err(InviteError::Invalid);
        }
        Ok(Self {
            addrs,
            port,
            short_fingerprint: short_fingerprint.to_owned(),
            window_random: fixed(one("r")?)?,
            secret: fixed(one("s")?)?,
        })
    }
}

fn fixed<const N: usize>(text: &str) -> Result<[u8; N], InviteError> {
    let bytes = base32::decode(text).map_err(|_| InviteError::Invalid)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| InviteError::Invalid)
}

/// A random code of `digits` decimal digits (6 for sync, 8 for pairing), uniformly distributed.
///
/// # Errors
///
/// Returns an error when the system has no random bytes.
pub fn new_code(digits: u32) -> Result<Zeroizing<String>, getrandom::Error> {
    let modulus = 10u64.pow(digits);
    // Reject the top of the range that would bias the remainder.
    let limit = u64::MAX - u64::MAX % modulus;
    loop {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes)?;
        let value = u64::from_be_bytes(bytes);
        if value < limit {
            let width = usize::try_from(digits).unwrap_or(8);
            return Ok(Zeroizing::new(format!("{:0width$}", value % modulus)));
        }
    }
}

/// A random 128-bit value: a pairing secret or a window's random value.
///
/// # Errors
///
/// Returns an error when the system has no random bytes.
pub fn new_random() -> Result<[u8; 16], getrandom::Error> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;

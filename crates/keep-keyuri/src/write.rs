//! Writing canonical `otpauth://` links.

use std::fmt::Write as _;

use keep_passcode::{CodeKind, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, base32};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use zeroize::Zeroizing;

use crate::KeyUri;

/// RFC 3986 unreserved characters stay as they are.
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
/// In the label, `@` also stays, so `alex@example.com` reads naturally; `:` inside a name is encoded so the
/// literal `:` between issuer and account is unambiguous.
const LABEL_PART: &AsciiSet = &QUERY_VALUE.remove(b'@');

/// Writes the canonical link for `uri`: parameters with default values are left out.
///
/// The link contains the secret, so it is wiped from memory when dropped.
#[must_use]
pub fn write_key_uri(uri: &KeyUri) -> Zeroizing<String> {
    let (kind, period_s) = match uri.spec.kind() {
        // Steam gets its own type: an app that does not know it refuses the link instead of showing wrong
        // decimal codes, which is what an unknown `encoder=steam` would give.
        CodeKind::TimeBased { period_s } if uri.spec.is_steam() => ("steam", Some(period_s)),
        CodeKind::TimeBased { period_s } => ("totp", Some(period_s)),
        CodeKind::CounterBased => ("hotp", None),
    };
    let mut link = Zeroizing::new(format!("otpauth://{kind}/{}", label(uri)));
    let secret = base32::encode(uri.secret.expose());
    link.push_str("?secret=");
    link.push_str(&secret);
    // Writing to a String cannot fail.
    if !uri.issuer.is_empty() {
        let _ = write!(
            link,
            "&issuer={}",
            utf8_percent_encode(&uri.issuer, QUERY_VALUE)
        );
    }
    if let Some(name) = digest_name(uri.spec.digest()) {
        let _ = write!(link, "&algorithm={name}");
    }
    if uri.spec.digits() != DEFAULT_DIGITS && !uri.spec.is_steam() {
        let _ = write!(link, "&digits={}", uri.spec.digits());
    }
    match period_s {
        Some(period_s) if period_s != DEFAULT_PERIOD_S => {
            let _ = write!(link, "&period={period_s}");
        }
        Some(_) => {}
        None => {
            let _ = write!(link, "&counter={}", uri.counter);
        }
    }
    link
}

/// `issuer:account`, or just `account`. An account containing `:` always gets the separator, so it is not
/// mistaken for an issuer when read back.
fn label(uri: &KeyUri) -> String {
    let account = utf8_percent_encode(&uri.account, LABEL_PART);
    if uri.issuer.is_empty() && !uri.account.contains(':') {
        return account.to_string();
    }
    format!("{}:{account}", utf8_percent_encode(&uri.issuer, LABEL_PART))
}

const fn digest_name(digest: Digest) -> Option<&'static str> {
    match digest {
        Digest::Sha1 => None,
        Digest::Sha256 => Some("SHA256"),
        Digest::Sha512 => Some("SHA512"),
        Digest::Md5 => Some("MD5"),
    }
}

#[cfg(test)]
mod tests;

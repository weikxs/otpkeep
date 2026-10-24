//! Reading `otpauth://TYPE/LABEL?PARAMS` links as people paste them.

use keep_passcode::base32::{self, Base32Fault};
use keep_passcode::{
    CodeKind, CodeSpec, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, MAX_DIGITS, MAX_PERIOD_S,
    MIN_DIGITS, MIN_PERIOD_S, STEAM_DIGITS, SecretKey,
};
use percent_encoding::percent_decode_str;
use zeroize::Zeroizing;

use crate::clean::{fold_width, locate, without_invisible};
use crate::{KeyUri, KeyUriFault, KeyUriNote};

const SCHEME: &str = "otpauth";
/// The Steam Guard link type, encoder value and issuer.
const STEAM: &str = "steam";
const FULL_WIDTH_COLON: char = '\u{ff1a}';
const ENCODED_COLON: &str = "%3a";

/// A parsed link plus the things worth telling the user about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedKeyUri {
    /// What the link says.
    pub uri: KeyUri,
    /// Non-fatal observations, such as a weak secret.
    pub notes: Vec<KeyUriNote>,
}

/// Parses an `otpauth://` link.
///
/// Lenient about how links arrive: invisible characters, words, quotes or brackets around the link and
/// full-width digits in values are tolerated; names are percent-decoded, unknown parameters are ignored and
/// the first of repeated parameters wins.
///
/// # Errors
///
/// Returns the [`KeyUriFault`] that makes the link unusable.
pub fn parse_key_uri(text: &str) -> Result<ParsedKeyUri, KeyUriFault> {
    let visible = without_invisible(text);
    let (kind_text, label, query) = split_link(locate(&visible)?)?;
    let mut notes = Vec::new();
    let params = Params::read(query, &mut notes);
    let (label_issuer, account) = split_label(label);
    let issuer = choose_issuer(params.get("issuer"), label_issuer, &mut notes);

    let secret = read_secret(params.get("secret"))?;
    let (spec, counter) = read_spec(kind_text, &params, &issuer, &mut notes)?;

    if secret.is_short() {
        notes.push(KeyUriNote::ShortSecret);
    }
    Ok(ParsedKeyUri {
        uri: KeyUri {
            secret,
            spec,
            counter,
            issuer,
            account,
        },
        notes,
    })
}

/// Splits a trimmed link into kind, raw label and raw query; the fragment is dropped.
fn split_link(text: &str) -> Result<(&str, &str, &str), KeyUriFault> {
    let (scheme, rest) = text.split_once("://").ok_or(KeyUriFault::NotAUri)?;
    let is_scheme = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !is_scheme {
        return Err(KeyUriFault::NotAUri);
    }
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return Err(KeyUriFault::WrongScheme {
            found: scheme.to_owned(),
        });
    }
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (kind, label) = path.split_once('/').unwrap_or((path, ""));
    if kind.is_empty() {
        return Err(KeyUriFault::NotAUri);
    }
    Ok((kind, label, query))
}

/// Splits `issuer:account`. A literal colon wins; a full-width colon is next; an encoded `%3A` is the last resort,
/// because writers that use a literal separator encode colons inside the names.
fn split_label(raw: &str) -> (String, String) {
    let split = raw
        .split_once(':')
        .or_else(|| raw.split_once(FULL_WIDTH_COLON))
        .or_else(|| {
            let at = raw.to_ascii_lowercase().find(ENCODED_COLON)?;
            Some((&raw[..at], &raw[at + ENCODED_COLON.len()..]))
        });
    match split {
        Some((issuer, account)) => (decode_path(issuer), decode_path(account)),
        None => (String::new(), decode_path(raw)),
    }
}

fn choose_issuer(param: Option<&str>, in_label: String, notes: &mut Vec<KeyUriNote>) -> String {
    let param = param.map(str::trim).filter(|value| !value.is_empty());
    match param {
        Some(issuer) => {
            if !in_label.is_empty() && in_label.to_lowercase() != issuer.to_lowercase() {
                notes.push(KeyUriNote::IssuerMismatch { in_label });
            }
            issuer.to_owned()
        }
        None => in_label,
    }
}

/// Reads a secret as people type it into a form: Base32 in any case, with spaces or dashes, with or without
/// padding, full-width characters from a Chinese input method, invisible characters from copying.
///
/// # Errors
///
/// Returns [`KeyUriFault::SecretMissing`] for an empty text and [`KeyUriFault::SecretInvalid`] with the
/// position of the first character that is not Base32.
pub fn parse_secret(text: &str) -> Result<SecretKey, KeyUriFault> {
    read_secret(Some(&without_invisible(text)))
}

fn read_secret(value: Option<&str>) -> Result<SecretKey, KeyUriFault> {
    let folded = fold_width(value.ok_or(KeyUriFault::SecretMissing)?);
    let bytes = base32::decode(&folded).map_err(|fault| match fault {
        Base32Fault::Empty => KeyUriFault::SecretMissing,
        Base32Fault::InvalidChar { at, found } => KeyUriFault::SecretInvalid { at, found },
        Base32Fault::MisplacedPadding { at } => KeyUriFault::SecretInvalid { at, found: '=' },
    })?;
    SecretKey::from_bytes(bytes.to_vec()).map_err(|_| KeyUriFault::SecretMissing)
}

/// How codes are computed, and the HOTP counter (0 for TOTP).
fn read_spec(
    kind_text: &str,
    params: &Params,
    issuer: &str,
    notes: &mut Vec<KeyUriNote>,
) -> Result<(CodeSpec, u64), KeyUriFault> {
    let kind = kind_text.to_ascii_lowercase();
    if kind == "hotp" {
        let counter = read_counter(params.get("counter"), notes)?;
        return Ok((decimal_spec(CodeKind::CounterBased, params)?, counter));
    }
    if kind != "totp" && kind != STEAM {
        return Err(KeyUriFault::UnsupportedKind {
            found: kind_text.to_owned(),
        });
    }
    let range = MIN_PERIOD_S..=MAX_PERIOD_S;
    let period_s = read_number("period", params.get("period"), DEFAULT_PERIOD_S, range)?;
    if is_steam(&kind, params, issuer) {
        return Ok((steam_spec(period_s, params)?, 0));
    }
    Ok((decimal_spec(CodeKind::TimeBased { period_s }, params)?, 0))
}

/// Steam Guard, in the three ways apps write it: the `steam` link type (Aegis, 2FAuth, Ente), `encoder=steam`
/// (KeePassXC, steamguard-cli, Strongbox), or a `Steam` issuer without other digits (FreeOTP).
fn is_steam(kind: &str, params: &Params, issuer: &str) -> bool {
    let encoder = params
        .get("encoder")
        .is_some_and(|e| e.trim().eq_ignore_ascii_case(STEAM));
    let by_issuer = issuer.trim().eq_ignore_ascii_case(STEAM)
        && params
            .get("digits")
            .is_none_or(|d| fold_width(d).trim() == STEAM_DIGITS.to_string());
    kind == STEAM || encoder || by_issuer
}

/// Steam codes are always SHA-1 with five characters; a link saying otherwise is refused, not guessed at.
fn steam_spec(period_s: u16, params: &Params) -> Result<CodeSpec, KeyUriFault> {
    if read_digest(params.get("algorithm"))? != Digest::Sha1 {
        return Err(KeyUriFault::UnsupportedAlgorithm {
            found: params.get("algorithm").unwrap_or_default().to_owned(),
        });
    }
    if let Some(digits) = params.get("digits")
        && fold_width(digits).trim() != STEAM_DIGITS.to_string()
    {
        return Err(out_of_range("digits", digits));
    }
    CodeSpec::steam(period_s).map_err(|_| out_of_range("period", &period_s.to_string()))
}

fn decimal_spec(kind: CodeKind, params: &Params) -> Result<CodeSpec, KeyUriFault> {
    let digest = read_digest(params.get("algorithm"))?;
    let range = MIN_DIGITS..=MAX_DIGITS;
    let digits = read_number("digits", params.get("digits"), DEFAULT_DIGITS, range)?;
    CodeSpec::new(kind, digest, digits).map_err(|_| out_of_range("digits", &digits.to_string()))
}

/// An HOTP counter; a link without one starts at 0, as FreeOTP and others do.
fn read_counter(value: Option<&str>, notes: &mut Vec<KeyUriNote>) -> Result<u64, KeyUriFault> {
    let Some(value) = value else {
        notes.push(KeyUriNote::CounterAssumedZero);
        return Ok(0);
    };
    fold_width(value)
        .trim()
        .parse::<u64>()
        .map_err(|_| out_of_range("counter", value))
}

fn read_digest(value: Option<&str>) -> Result<Digest, KeyUriFault> {
    let Some(value) = value else {
        return Ok(Digest::Sha1);
    };
    let mut name = fold_width(value).trim().to_ascii_uppercase();
    name.retain(|c| !matches!(c, '-' | '_' | ' '));
    match name.strip_prefix("HMAC").unwrap_or(&name) {
        "SHA1" => Ok(Digest::Sha1),
        "SHA256" => Ok(Digest::Sha256),
        "SHA512" => Ok(Digest::Sha512),
        "MD5" => Ok(Digest::Md5),
        _ => Err(KeyUriFault::UnsupportedAlgorithm {
            found: value.to_owned(),
        }),
    }
}

fn read_number<T>(
    name: &'static str,
    value: Option<&str>,
    default: T,
    range: std::ops::RangeInclusive<T>,
) -> Result<T, KeyUriFault>
where
    T: std::str::FromStr + PartialOrd,
{
    let Some(value) = value else {
        return Ok(default);
    };
    match fold_width(value).trim().parse::<T>() {
        Ok(number) if range.contains(&number) => Ok(number),
        _ => Err(out_of_range(name, value)),
    }
}

fn out_of_range(name: &'static str, value: &str) -> KeyUriFault {
    KeyUriFault::ParamOutOfRange {
        name,
        value: value.to_owned(),
    }
}

fn decode_path(raw: &str) -> String {
    percent_decode_str(raw)
        .decode_utf8_lossy()
        .trim()
        .to_owned()
}

/// Query parameters: names are case-insensitive, `+` means a space, the first of repeated names wins.
/// Values are wiped from memory when dropped, because one of them is the secret.
struct Params(Vec<(String, Zeroizing<String>)>);

impl Params {
    fn read(query: &str, notes: &mut Vec<KeyUriNote>) -> Self {
        let mut pairs: Vec<(String, Zeroizing<String>)> = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let name = decode_query(name).to_lowercase();
            if pairs.iter().any(|(seen, _)| *seen == name) {
                let note = KeyUriNote::DuplicateParam { name };
                if !notes.contains(&note) {
                    notes.push(note);
                }
                continue;
            }
            pairs.push((name, decode_query(value)));
        }
        Self(pairs)
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn decode_query(raw: &str) -> Zeroizing<String> {
    let spaced = Zeroizing::new(raw.replace('+', " "));
    Zeroizing::new(percent_decode_str(&spaced).decode_utf8_lossy().into_owned())
}

#[cfg(test)]
mod lenient_tests;
#[cfg(test)]
mod steam_tests;
#[cfg(test)]
mod tests;

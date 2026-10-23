//! Reading `otpauth://TYPE/LABEL?PARAMS` links as people paste them.

use keep_passcode::base32::{self, Base32Fault};
use keep_passcode::{
    CodeKind, CodeSpec, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, MAX_DIGITS, MAX_PERIOD_S,
    MIN_DIGITS, MIN_PERIOD_S, SecretKey,
};
use percent_encoding::percent_decode_str;
use zeroize::Zeroizing;

use crate::{KeyUri, KeyUriFault, KeyUriNote};

const SCHEME: &str = "otpauth";
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
/// Surrounding whitespace is ignored, names are percent-decoded, unknown parameters are ignored and the
/// first of repeated parameters wins.
///
/// # Errors
///
/// Returns the [`KeyUriFault`] that makes the link unusable.
pub fn parse_key_uri(text: &str) -> Result<ParsedKeyUri, KeyUriFault> {
    let (kind_text, label, query) = split_link(text.trim())?;
    let mut notes = Vec::new();
    let params = Params::read(query, &mut notes);
    let (label_issuer, account) = split_label(label);
    let issuer = choose_issuer(params.get("issuer"), label_issuer, &mut notes);

    let secret = read_secret(params.get("secret"))?;
    let digest = read_digest(params.get("algorithm"))?;
    let digits = read_number(
        "digits",
        params.get("digits"),
        DEFAULT_DIGITS,
        MIN_DIGITS..=MAX_DIGITS,
    )?;
    let (kind, counter) = match kind_text.to_ascii_lowercase().as_str() {
        "totp" => {
            let range = MIN_PERIOD_S..=MAX_PERIOD_S;
            let period_s = read_number("period", params.get("period"), DEFAULT_PERIOD_S, range)?;
            (CodeKind::TimeBased { period_s }, 0)
        }
        "hotp" => {
            let value = params.get("counter").ok_or(KeyUriFault::CounterMissing)?;
            let counter = value
                .trim()
                .parse::<u64>()
                .map_err(|_| out_of_range("counter", value))?;
            (CodeKind::CounterBased, counter)
        }
        _ => {
            return Err(KeyUriFault::UnsupportedKind {
                found: kind_text.to_owned(),
            });
        }
    };
    let spec = CodeSpec::new(kind, digest, digits)
        .map_err(|_| out_of_range("digits", &digits.to_string()))?;

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

fn read_secret(value: Option<&str>) -> Result<SecretKey, KeyUriFault> {
    let bytes =
        base32::decode(value.ok_or(KeyUriFault::SecretMissing)?).map_err(|fault| match fault {
            Base32Fault::Empty => KeyUriFault::SecretMissing,
            Base32Fault::InvalidChar { at, found } => KeyUriFault::SecretInvalid { at, found },
            Base32Fault::MisplacedPadding { at } => KeyUriFault::SecretInvalid { at, found: '=' },
        })?;
    SecretKey::from_bytes(bytes.to_vec()).map_err(|_| KeyUriFault::SecretMissing)
}

fn read_digest(value: Option<&str>) -> Result<Digest, KeyUriFault> {
    let Some(value) = value else {
        return Ok(Digest::Sha1);
    };
    match value.trim().to_ascii_uppercase().replace('-', "").as_str() {
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
    match value.trim().parse::<T>() {
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
mod tests;

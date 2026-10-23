//! Reading GoldenPassport data, read-only (PRD FR-6.4, design/05-transfer.md §6).
//!
//! Formats, from the GoldenPassport sources and compatibility notes (design/10-reference.md V5):
//! - 0.1.x `gp.secrets` and exported `.secrets`: an `NSKeyedArchiver` binary plist of a dictionary from display
//!   name to `otpauth://` link;
//! - 0.2.x `accounts.json`: a JSON array of `{id, name, url}` (a future `{"formatVersion": 2, "accounts": […]}`
//!   layout is read too);
//! - 0.2.x `.txt` exports are link sheets and are read by `keep_keyuri::read_sheet`.
//!
//! Both live in `~/Library/Application Support/GoldenPassport/`. They are opened only after the user agrees.

use keep_keyuri::{KeyUri, KeyUriFault, parse_key_uri, write_key_uri};
use keep_passcode::{CodeKind, Digest};
use plist::{Dictionary, Uid, Value};
use zeroize::Zeroizing;

use crate::import::{Candidate, Origin};

/// GoldenPassport's data directory, relative to the user's home.
pub const DATA_DIR_IN_HOME: &str = "Library/Application Support/GoldenPassport";
/// The 0.1.x data file.
pub const SECRETS_FILE: &str = "gp.secrets";
/// The 0.2.x data file.
pub const ACCOUNTS_FILE: &str = "accounts.json";

/// The file is not GoldenPassport data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GpError {
    /// Not a format GoldenPassport writes.
    #[error("this is not a GoldenPassport data file")]
    NotRecognized,
}

/// An account whose link could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpProblem {
    /// Its name in GoldenPassport.
    pub name: String,
    /// Why the link is unusable.
    pub fault: KeyUriFault,
}

/// What was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpRead {
    /// Accounts ready for the import preview.
    pub candidates: Vec<Candidate>,
    /// Accounts skipped.
    pub problems: Vec<GpProblem>,
}

/// Reads a 0.1.x `gp.secrets` or an exported `.secrets` file. The dictionary has no order, so accounts are
/// sorted by name.
///
/// # Errors
///
/// Returns [`GpError::NotRecognized`] for anything but a keyed archive of names and links.
pub fn read_secrets(bytes: &[u8]) -> Result<GpRead, GpError> {
    let archive =
        Value::from_reader(std::io::Cursor::new(bytes)).map_err(|_| GpError::NotRecognized)?;
    let mut pairs = keyed_dictionary(&archive).ok_or(GpError::NotRecognized)?;
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(read_pairs(pairs))
}

/// Reads a 0.2.x `accounts.json`, keeping its order.
///
/// # Errors
///
/// Returns [`GpError::NotRecognized`] for JSON that is not a list of accounts.
pub fn read_accounts_json(bytes: &[u8]) -> Result<GpRead, GpError> {
    let json: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| GpError::NotRecognized)?;
    let list = match &json {
        serde_json::Value::Array(list) => list,
        serde_json::Value::Object(object) => match object.get("accounts") {
            Some(serde_json::Value::Array(list)) => list,
            _ => return Err(GpError::NotRecognized),
        },
        _ => return Err(GpError::NotRecognized),
    };
    let pairs = list
        .iter()
        .map(|account| {
            let name = account.get("name")?.as_str()?.to_owned();
            let url = Zeroizing::new(account.get("url")?.as_str()?.to_owned());
            Some((name, url))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(GpError::NotRecognized)?;
    Ok(read_pairs(pairs))
}

fn read_pairs(pairs: Vec<(String, Zeroizing<String>)>) -> GpRead {
    let mut read = GpRead {
        candidates: Vec::new(),
        problems: Vec::new(),
    };
    for (index, (name, url)) in pairs.into_iter().enumerate() {
        match parse_key_uri(&url) {
            Ok(parsed) => read.candidates.push(Candidate {
                title: name,
                issuer: parsed.uri.issuer,
                account: parsed.uri.account,
                secret: parsed.uri.secret,
                spec: parsed.uri.spec,
                counter: parsed.uri.counter,
                note: String::new(),
                tags: Vec::new(),
                origin: Origin::Item(index),
            }),
            Err(fault) => read.problems.push(GpProblem { name, fault }),
        }
    }
    read
}

/// The `{name: url}` pairs of a keyed archive whose root is a dictionary of strings.
fn keyed_dictionary(archive: &Value) -> Option<Vec<(String, Zeroizing<String>)>> {
    let archive = archive.as_dictionary()?;
    let objects = archive.get("$objects")?.as_array()?;
    let root_uid = archive
        .get("$top")?
        .as_dictionary()?
        .get("root")?
        .as_uid()?;
    let root = object(objects, root_uid.get())?.as_dictionary()?;
    let keys = root.get("NS.keys")?.as_array()?;
    let values = root.get("NS.objects")?.as_array()?;
    if keys.len() != values.len() {
        return None;
    }
    keys.iter()
        .zip(values)
        .map(|(key, value)| {
            let name = string(objects, key)?;
            let url = Zeroizing::new(string(objects, value)?);
            Some((name, url))
        })
        .collect()
}

fn object(objects: &[Value], uid: u64) -> Option<&Value> {
    objects.get(usize::try_from(uid).ok()?)
}

/// A string object: stored directly, or as a mutable string `{NS.string: …}`.
fn string(objects: &[Value], reference: &Value) -> Option<String> {
    match object(objects, reference.as_uid()?.get())? {
        Value::String(text) => Some(text.clone()),
        Value::Dictionary(d) => mutable_string(d),
        _ => None,
    }
}

fn mutable_string(dictionary: &Dictionary) -> Option<String> {
    dictionary.get("NS.string")?.as_string().map(str::to_owned)
}

/// One account to export for GoldenPassport.
#[derive(Debug, Clone, Copy)]
pub struct GpExportItem<'a> {
    /// The name GoldenPassport shows; made unique and never empty.
    pub name: &'a str,
    /// The account.
    pub uri: &'a KeyUri,
}

/// Why an account is left out of a GoldenPassport file: GoldenPassport 0.1.x reads only the secret and always
/// computes 6-digit SHA-1 codes every 30 seconds, so it would show wrong codes for these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpIncompatible {
    /// HOTP.
    CounterBased,
    /// Not SHA-1.
    Algorithm,
    /// Not 6 digits.
    Digits,
    /// Not 30 seconds.
    Period,
}

/// A GoldenPassport `.secrets` file. It holds every secret unencrypted and is wiped when dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpExport {
    /// The file contents.
    pub bytes: Zeroizing<Vec<u8>>,
    /// Accounts left out, by input position.
    pub skipped: Vec<(usize, GpIncompatible)>,
}

/// Writes a `.secrets` file that GoldenPassport 0.1.x (and later) can import: a binary keyed archive of an
/// `NSDictionary` from name to link, laid out as `NSKeyedArchiver` lays it out.
#[must_use]
pub fn write_secrets(items: &[GpExportItem<'_>]) -> GpExport {
    let mut skipped = Vec::new();
    let mut pairs: Vec<(String, Zeroizing<String>)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(reason) = gp_incompatibility(item.uri) {
            skipped.push((index, reason));
            continue;
        }
        let name = unique_name(&display_name(item), &pairs);
        pairs.push((name, write_key_uri(item.uri)));
    }
    let mut bytes = Zeroizing::new(Vec::new());
    // Serializing an in-memory value into a Vec cannot fail.
    let _ = keyed_archive(&pairs).to_writer_binary(&mut *bytes);
    GpExport { bytes, skipped }
}

fn gp_incompatibility(uri: &KeyUri) -> Option<GpIncompatible> {
    match uri.spec.kind() {
        CodeKind::CounterBased => Some(GpIncompatible::CounterBased),
        _ if uri.spec.digest() != Digest::Sha1 => Some(GpIncompatible::Algorithm),
        _ if uri.spec.digits() != 6 => Some(GpIncompatible::Digits),
        CodeKind::TimeBased { period_s } if period_s != 30 => Some(GpIncompatible::Period),
        CodeKind::TimeBased { .. } => None,
    }
}

fn display_name(item: &GpExportItem<'_>) -> String {
    [
        item.name,
        item.uri.issuer.as_str(),
        item.uri.account.as_str(),
    ]
    .into_iter()
    .map(str::trim)
    .find(|n| !n.is_empty())
    .unwrap_or("OTP")
    .to_owned()
}

/// `name`, or `name (2)`, `name (3)`, … when the dictionary already has it. With `n` names taken, one of
/// `name (2)` … `name (n + 2)` is always free.
fn unique_name(name: &str, taken: &[(String, Zeroizing<String>)]) -> String {
    let free = |candidate: &str| !taken.iter().any(|(n, _)| n == candidate);
    if free(name) {
        return name.to_owned();
    }
    (2..=taken.len() + 2)
        .map(|n| format!("{name} ({n})"))
        .find(|c| free(c))
        .unwrap_or_else(|| name.to_owned())
}

/// The keyed-archive layout: `$null`, the dictionary, its keys, its values, then the class description.
fn keyed_archive(pairs: &[(String, Zeroizing<String>)]) -> Value {
    let uid = |index: usize| Value::Uid(Uid::new(index as u64));
    let first_key = 2;
    let first_value = first_key + pairs.len();
    let class = first_value + pairs.len();
    let mut root = Dictionary::new();
    root.insert(
        "NS.keys".into(),
        Value::Array((first_key..first_value).map(uid).collect()),
    );
    root.insert(
        "NS.objects".into(),
        Value::Array((first_value..class).map(uid).collect()),
    );
    root.insert("$class".into(), uid(class));
    let mut class_info = Dictionary::new();
    class_info.insert("$classname".into(), Value::String("NSDictionary".into()));
    class_info.insert(
        "$classes".into(),
        Value::Array(vec![
            Value::String("NSDictionary".into()),
            Value::String("NSObject".into()),
        ]),
    );
    let mut objects = vec![Value::String("$null".into()), Value::Dictionary(root)];
    objects.extend(pairs.iter().map(|(name, _)| Value::String(name.clone())));
    objects.extend(
        pairs
            .iter()
            .map(|(_, link)| Value::String(link.to_string())),
    );
    objects.push(Value::Dictionary(class_info));
    let mut top = Dictionary::new();
    top.insert("root".into(), uid(1));
    let mut archive = Dictionary::new();
    archive.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
    archive.insert("$version".into(), Value::Integer(100_000.into()));
    archive.insert("$top".into(), Value::Dictionary(top));
    archive.insert("$objects".into(), Value::Array(objects));
    Value::Dictionary(archive)
}

#[cfg(test)]
mod tests;

//! The key behind an entry: its parameters, editing them and replacing the secret (PRD FR-4.4). The window
//! sees the parameters and codes; a new secret only travels in, typed by the person.

use keep_keyuri::{KeyUriFault, parse_secret};
use keep_passcode::{CodeKind, CodeSpec, Digest, Moment, SecretKey, derive_passcode};
use keep_vault::edit::Change;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::{Core, CoreError, Vault, find};

const MS_PER_S: u64 = 1000;

/// An entry's code parameters, without the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub kind: &'static str,
    pub algorithm: &'static str,
    pub digits: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_s: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter: Option<u64>,
    /// The secret is shorter than 10 bytes (PRD FR-2.8).
    pub short_secret: bool,
}

/// New parameters, and a new secret when one is typed (empty keeps the current secret).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyForm {
    #[serde(default)]
    pub secret: String,
    pub kind: String,
    pub algorithm: String,
    pub digits: u8,
    pub period_s: u16,
    #[serde(default)]
    pub counter: u64,
}

impl Drop for KeyForm {
    /// A typed secret is wiped when the form is dropped.
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

/// A code specification from the form's fields; Steam ignores the algorithm and digits.
///
/// # Errors
///
/// Returns [`CoreError::Invalid`] naming the field (`KIND`, `ALGORITHM`, `DIGITS`, `PERIOD`).
pub fn read_spec(
    kind: &str,
    algorithm: &str,
    digits: u8,
    period_s: u16,
) -> Result<CodeSpec, CoreError> {
    if kind == "steam" {
        return CodeSpec::steam(period_s).map_err(|_| CoreError::Invalid("PERIOD"));
    }
    let digest = match algorithm.trim().to_ascii_uppercase().as_str() {
        "SHA1" => Digest::Sha1,
        "SHA256" => Digest::Sha256,
        "SHA512" => Digest::Sha512,
        _ => return Err(CoreError::Invalid("ALGORITHM")),
    };
    let code_kind = match kind {
        "totp" => CodeKind::TimeBased { period_s },
        "hotp" => CodeKind::CounterBased,
        _ => return Err(CoreError::Invalid("KIND")),
    };
    CodeSpec::new(code_kind, digest, digits).map_err(|error| match error {
        keep_passcode::SpecError::DigitsOutOfRange(_) => CoreError::Invalid("DIGITS"),
        keep_passcode::SpecError::PeriodOutOfRange(_) => CoreError::Invalid("PERIOD"),
    })
}

/// A secret as typed, read as leniently as secrets in links.
///
/// # Errors
///
/// Returns [`CoreError::SecretInvalid`] with the position of a bad character, or `SECRET_MISSING`.
pub fn read_typed_secret(text: &str) -> Result<SecretKey, CoreError> {
    parse_secret(text).map_err(|fault| match fault {
        KeyUriFault::SecretInvalid { at, found } => CoreError::SecretInvalid { at, found },
        _ => CoreError::Invalid("SECRET_MISSING"),
    })
}

const fn kind_name(spec: CodeSpec) -> &'static str {
    match spec.kind() {
        CodeKind::TimeBased { .. } if spec.is_steam() => "steam",
        CodeKind::TimeBased { .. } => "totp",
        CodeKind::CounterBased => "hotp",
    }
}

const fn algorithm_name(digest: Digest) -> &'static str {
    match digest {
        Digest::Sha1 => "SHA1",
        Digest::Sha256 => "SHA256",
        Digest::Sha512 => "SHA512",
        Digest::Md5 => "MD5",
    }
}

impl Core {
    /// The parameters of an entry's key.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the data is not open or the entry is gone.
    pub fn key_info(&self, id: &str) -> Result<KeyInfo, CoreError> {
        let Vault::Open { service, .. } = &self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        let entry = service
            .entry(find(service, id)?)
            .ok_or(CoreError::NotFound)?;
        let key = &entry.key.value;
        let (period_s, counter) = match key.spec.kind() {
            CodeKind::TimeBased { period_s } => (Some(period_s), None),
            CodeKind::CounterBased => (None, Some(key.counter)),
        };
        Ok(KeyInfo {
            kind: kind_name(key.spec),
            algorithm: algorithm_name(key.spec.digest()),
            digits: key.spec.digits(),
            period_s,
            counter,
            short_secret: key.secret.is_short(),
        })
    }

    /// The code the entry would show right now with these parameters, to compare with the website before
    /// saving; `None` for counter codes, which would move on.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] for a field that is not acceptable.
    pub fn preview_key(&self, id: &str, form: &KeyForm) -> Result<Option<String>, CoreError> {
        let (secret, spec, _) = self.read_key(id, form)?;
        if spec.kind() == CodeKind::CounterBased {
            return Ok(None);
        }
        let seconds = i64::try_from((self.clock)() / MS_PER_S).unwrap_or(i64::MAX);
        Ok(derive_passcode(&secret, spec, Moment::Time(seconds))
            .ok()
            .map(|p| p.code().to_owned()))
    }

    /// Saves new parameters and, if one was typed, a new secret. The old secret stops producing codes here;
    /// a copy of it is kept with the entry's history (design/02-data).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing changes on failure.
    pub fn replace_key(&mut self, id: &str, form: &KeyForm) -> Result<(), CoreError> {
        let (secret, spec, counter) = self.read_key(id, form)?;
        let service = self.writable()?;
        let entry = find(service, id)?;
        Ok(service.replace_key(entry, secret, spec, counter)?)
    }

    /// Changes the service and / or account name.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] with `ISSUER_TOO_LONG` or `ACCOUNT_TOO_LONG`.
    pub fn rename(
        &mut self,
        id: &str,
        issuer: Option<String>,
        account: Option<String>,
    ) -> Result<(), CoreError> {
        let service = self.writable()?;
        let entry = find(service, id)?;
        let changes = [issuer.map(Change::Issuer), account.map(Change::Account)]
            .into_iter()
            .flatten()
            .collect();
        Ok(service.edit_all(entry, changes)?)
    }

    fn read_key(&self, id: &str, form: &KeyForm) -> Result<(SecretKey, CodeSpec, u64), CoreError> {
        let spec = read_spec(&form.kind, &form.algorithm, form.digits, form.period_s)?;
        let secret = if form.secret.trim().is_empty() {
            let Vault::Open { service, .. } = &self.vault else {
                return Err(CoreError::NeedsPassword);
            };
            let entry = service
                .entry(find(service, id)?)
                .ok_or(CoreError::NotFound)?;
            entry.key.value.secret.clone()
        } else {
            read_typed_secret(&form.secret)?
        };
        let counter = if spec.kind() == CodeKind::CounterBased {
            form.counter
        } else {
            0
        };
        Ok((secret, spec, counter))
    }
}

#[cfg(test)]
mod tests;

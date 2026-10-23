//! `otpkeep code` and `otpkeep list`.

use std::fmt::Write as _;

use keep_find::{FindContext, FindIndex};
use keep_passcode::{CodeKind, Passcode};
use keep_vault::open::SaveError;
use keep_vault::service::{ServiceError, VaultService};
use keep_vault::{Entry, EntryId};
use serde_json::{Value, json};

use crate::{
    CliError, EXIT_INVALID_INPUT, EXIT_MULTIPLE_MATCHES, EXIT_NO_MATCH, EXIT_READ_ONLY,
    EXIT_VAULT_ERROR,
};

/// What a command printed, as plain text and as the `data` of the JSON form.
pub(crate) struct Printed {
    pub(crate) text: String,
    pub(crate) json: Value,
    /// Warnings for standard error, such as accounts that were skipped.
    pub(crate) notes: Vec<String>,
}

/// The passcode of the one entry `query` names (PRD FR-11.1, FR-11.4).
pub(crate) fn code(
    service: &mut VaultService,
    query: &[String],
    next: bool,
) -> Result<Printed, CliError> {
    let id = resolve(service, &query.join(" "))?;
    let entry = service.entry(id).ok_or_else(|| missing(&query.join(" ")))?;
    let title = entry.title.value.clone();
    let passcode = match entry.key.value.spec.kind() {
        CodeKind::TimeBased { .. } => service.passcode(id).map_err(service_error)?,
        CodeKind::CounterBased if next => service.next_hotp(id).map_err(service_error)?,
        CodeKind::CounterBased => {
            return Err(CliError::new(
                EXIT_INVALID_INPUT,
                "INVALID_INPUT",
                "this is a counter-based (HOTP) entry; run again with --next to generate its next code",
            ));
        }
    };
    Ok(Printed {
        text: format!("{}\n", passcode.code()),
        json: passcode_json(&passcode, &title),
        notes: Vec::new(),
    })
}

/// Names, services and accounts, optionally of one tag; never codes or secrets.
pub(crate) fn list(service: &VaultService, tag: Option<&str>) -> Printed {
    let wanted = tag.map(str::to_lowercase);
    let entries: Vec<&Entry> = service
        .ordered()
        .into_iter()
        .filter(|e| {
            wanted
                .as_ref()
                .is_none_or(|t| e.tags.value.iter().any(|tag| tag.to_lowercase() == *t))
        })
        .collect();
    let text = entries.iter().fold(String::new(), |mut text, e| {
        // Writing to a String cannot fail.
        let _ = writeln!(
            text,
            "{}	{}	{}",
            e.title.value, e.issuer.value, e.account.value
        );
        text
    });
    let json = Value::Array(
        entries
            .iter()
            .map(|e| {
                json!({
                    "title": e.title.value,
                    "issuer": e.issuer.value,
                    "account": e.account.value,
                    "tags": e.tags.value,
                    "kind": if e.key.value.spec.kind() == CodeKind::CounterBased { "hotp" } else { "totp" },
                })
            })
            .collect(),
    );
    Printed {
        text,
        json,
        notes: Vec::new(),
    }
}

/// The one entry a search names: the only hit, or among several the only one whose name is the search.
fn resolve(service: &VaultService, query: &str) -> Result<EntryId, CliError> {
    let entries = service.ordered();
    let hits = FindIndex::build(&entries).find(query, &FindContext::default());
    match hits.as_slice() {
        [] => Err(missing(query)),
        [only] => Ok(only.entry),
        many => {
            let wanted = query.trim().to_lowercase();
            let exact: Vec<EntryId> = many
                .iter()
                .filter_map(|hit| service.entry(hit.entry))
                .filter(|e| e.title.value.to_lowercase() == wanted)
                .map(|e| e.id)
                .collect();
            if let [id] = exact.as_slice() {
                return Ok(*id);
            }
            let candidates: Vec<String> = many
                .iter()
                .filter_map(|hit| service.entry(hit.entry))
                .map(|e| {
                    format!(
                        "{} · {} · {}",
                        e.title.value, e.issuer.value, e.account.value
                    )
                })
                .collect();
            Err(CliError::with_candidates(
                EXIT_MULTIPLE_MATCHES,
                "MULTIPLE_MATCHES",
                &format!(
                    "'{query}' matches {} entries; be more specific",
                    candidates.len()
                ),
                candidates,
            ))
        }
    }
}

fn passcode_json(passcode: &Passcode, title: &str) -> Value {
    json!({ "code": passcode.code(), "valid_for_s": passcode.valid_for_s(), "title": title })
}

fn missing(query: &str) -> CliError {
    CliError::new(
        EXIT_NO_MATCH,
        "NO_MATCH",
        &format!("nothing matches '{query}'"),
    )
}

pub(crate) fn service_error(error: ServiceError) -> CliError {
    match error {
        ServiceError::Save(SaveError::ReadOnly) => CliError::new(
            EXIT_READ_ONLY,
            "READ_ONLY",
            "the data is read-only because a newer version of OTPKeep uses it",
        ),
        ServiceError::Passcode(_) => CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "no code can be computed for this entry (unsupported algorithm)",
        ),
        other => CliError::new(EXIT_VAULT_ERROR, "VAULT_ERROR", &other.to_string()),
    }
}

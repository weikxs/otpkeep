//! Codes for scripts, asked through the app's local socket (docs/scripting.md): the same search and the same
//! rules as `otpkeep code`. A code given to a script does not count as a recent use.

use keep_passcode::CodeKind;
use keep_platform::app_socket::Response;
use keep_vault::EntryId;

use super::{Core, CoreError, Problem, Vault, find};

impl Core {
    /// The code of the one entry `query` names; with `next`, a counter-based entry's next code (which is saved).
    pub fn script_code(&mut self, query: &str, next: bool) -> Response {
        let id = match self.resolve(query) {
            Ok(id) => id,
            Err(failure) => return failure,
        };
        let Vault::Open { service, .. } = &self.vault else {
            return locked();
        };
        let Some(entry) = service.entry(id) else {
            return not_found(query);
        };
        let name = entry.title.value.clone();
        let counter = entry.key.value.spec.kind() == CodeKind::CounterBased;
        if counter && !next {
            return Response::error(
                "NEEDS_NEXT",
                "this is a counter-based (HOTP) account; ask with --next to make its next code",
                Vec::new(),
            );
        }
        let made = if counter {
            self.writable()
                .and_then(|service| service.next_hotp(id).map_err(CoreError::from))
        } else {
            service.passcode(id).map_err(CoreError::from)
        };
        match made {
            Ok(passcode) => {
                Response::code(&name, passcode.code().to_owned(), passcode.valid_for_s())
            }
            Err(CoreError::ReadOnly) => Response::error(
                "READ_ONLY",
                "the data is read-only because a newer version of OTPKeep uses it",
                Vec::new(),
            ),
            Err(_) => Response::error(
                "FAILED",
                "no code can be computed for this account",
                Vec::new(),
            ),
        }
    }

    /// The one entry a search names: the only hit, or among several the only one whose name is the search.
    fn resolve(&self, query: &str) -> Result<EntryId, Response> {
        match &self.vault {
            Vault::Open { .. } => {}
            Vault::Fresh => return Err(not_found(query)),
            Vault::NeedsPassword => return Err(locked()),
            Vault::Unavailable(problem) => return Err(unavailable(*problem)),
        }
        let hits = self.list(query).map_err(|_| locked())?;
        let service = self.service().map_err(|_| locked())?;
        let id = |info: &super::EntryInfo| find(service, &info.id).ok();
        match hits.as_slice() {
            [] => Err(not_found(query)),
            [only] => id(only).ok_or_else(|| not_found(query)),
            many => {
                let wanted = query.trim().to_lowercase();
                let exact: Vec<&super::EntryInfo> = many
                    .iter()
                    .filter(|info| info.title.to_lowercase() == wanted)
                    .collect();
                if let [only] = exact.as_slice() {
                    return id(only).ok_or_else(|| not_found(query));
                }
                let candidates = many
                    .iter()
                    .map(|info| format!("{} · {} · {}", info.title, info.issuer, info.account))
                    .collect::<Vec<_>>();
                Err(Response::error(
                    "AMBIGUOUS",
                    &format!(
                        "'{query}' matches {} accounts; be more specific",
                        candidates.len()
                    ),
                    candidates,
                ))
            }
        }
    }
}

fn locked() -> Response {
    Response::error(
        "LOCKED",
        "OTPKeep is waiting for its master password; unlock it from the menu bar icon",
        Vec::new(),
    )
}

fn not_found(query: &str) -> Response {
    Response::error(
        "NOT_FOUND",
        &format!("nothing matches '{query}'"),
        Vec::new(),
    )
}

fn unavailable(problem: Problem) -> Response {
    Response::error(
        "UNAVAILABLE",
        &format!("OTPKeep cannot open its data ({problem:?}); see the app's menu"),
        Vec::new(),
    )
}

#[cfg(test)]
mod tests;

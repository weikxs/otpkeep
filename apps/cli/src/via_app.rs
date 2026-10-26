//! `otpkeep code` while the OTPKeep app runs: the app holds the vault, so the code is asked from it through its
//! local socket instead of waiting for the vault (docs/scripting.md). Output and exit codes are the same as when
//! the command line opens the vault itself.

use std::path::Path;

use keep_platform::app_socket::{self, AskError, Request, Response};
use serde_json::json;

use crate::commands::Printed;
use crate::{
    CliError, EXIT_INVALID_INPUT, EXIT_MULTIPLE_MATCHES, EXIT_NO_MATCH, EXIT_READ_ONLY,
    EXIT_SCRIPTS_DISABLED, EXIT_VAULT_ERROR,
};

/// What came of asking the app.
pub(crate) enum Asked {
    /// The app answered: a code or why there is none.
    Answer(Result<Printed, CliError>),
    /// No app answered, or it waits for its master password: open the vault directly.
    OpenDirectly,
}

/// Asks the app listening in `data_dir` for the code of the one entry `query` names.
pub(crate) fn ask(data_dir: &Path, query: &str, next: bool) -> Asked {
    match app_socket::ask(data_dir, &Request::code(query, next)) {
        Ok(response) => answer(response, query),
        Err(AskError::NotRunning | AskError::Failed(_)) => Asked::OpenDirectly,
    }
}

fn answer(response: Response, query: &str) -> Asked {
    let failure = match response {
        Response::Code(given) => {
            return Asked::Answer(Ok(Printed {
                text: format!("{}\n", given.code),
                json: json!({ "code": given.code, "valid_for_s": given.valid_for_s, "title": given.name }),
                notes: Vec::new(),
            }));
        }
        Response::Error(failure) => failure,
    };
    let error = match failure.error.as_str() {
        // A locked app does not hold the vault: the command line can open it with the password.
        "LOCKED" => return Asked::OpenDirectly,
        "NOT_FOUND" => CliError::new(
            EXIT_NO_MATCH,
            "NO_MATCH",
            &format!("nothing matches '{query}'"),
        ),
        "AMBIGUOUS" => CliError::with_candidates(
            EXIT_MULTIPLE_MATCHES,
            "MULTIPLE_MATCHES",
            &failure.message,
            failure.candidates,
        ),
        "NEEDS_NEXT" => CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "this is a counter-based (HOTP) entry; run again with --next to generate its next code",
        ),
        "READ_ONLY" => CliError::new(EXIT_READ_ONLY, "READ_ONLY", &failure.message),
        "DISABLED" => CliError::new(EXIT_SCRIPTS_DISABLED, "DISABLED", &failure.message),
        _ => CliError::new(EXIT_VAULT_ERROR, "VAULT_ERROR", &failure.message),
    };
    Asked::Answer(Err(error))
}

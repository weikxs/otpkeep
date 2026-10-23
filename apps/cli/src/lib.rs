//! The `otpkeep` command line (design/07-platform.md §2.6, exit codes in design/10-reference.md §4.2).
//!
//! When the app is not running, the command line opens the vault itself, taking the single-writer lock.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use keep_platform::Keystore;
use serde_json::{Value, json};

mod args;
mod commands;
mod open;

use args::{Command, Options, UsageError, parse};

/// Success.
pub const EXIT_OK: u8 = 0;
/// Nothing matched the search.
pub const EXIT_NO_MATCH: u8 = 2;
/// Several entries matched; the candidates are listed.
pub const EXIT_MULTIPLE_MATCHES: u8 = 3;
/// Locked, and unlocking was not possible.
pub const EXIT_LOCKED: u8 = 4;
/// Authentication failed or was cancelled.
pub const EXIT_AUTH_FAILED: u8 = 5;
/// Another process is using the vault.
pub const EXIT_BUSY: u8 = 6;
/// The vault cannot be opened.
pub const EXIT_VAULT_ERROR: u8 = 7;
/// The vault is read-only.
pub const EXIT_READ_ONLY: u8 = 8;
/// Cancelled at a confirmation.
pub const EXIT_CANCELLED: u8 = 9;
/// The input is not valid for this entry or command.
pub const EXIT_INVALID_INPUT: u8 = 10;
/// The command line could not be understood.
pub const EXIT_USAGE: u8 = 64;

/// How long to wait for another process to release the vault (design/10-reference.md §2).
pub const LOCK_WAIT: Duration = Duration::from_secs(5);

const NAME: &str = "otpkeep";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
Usage: otpkeep <command>

Commands:
  code <query>   Print the code of the one entry matching <query>
                   --next            for counter-based (HOTP) entries: generate the next code
  list           List entries (names only, never codes)
                   --tag <tag>       only entries with this tag
  version        Print the version
  help           Print this help

Options for code and list:
  --json             Print {\"ok\": …, \"data\" | \"error\": …}
  --password-stdin   Read the master password from the first line of standard input
";

/// What the command line needs from its surroundings; tests pass their own.
pub struct Context<'a> {
    /// The data directory, if one could be determined.
    pub data_dir: Option<PathBuf>,
    /// Where the data key is kept.
    pub keystore: &'a dyn Keystore,
    /// Standard input, for `--password-stdin`.
    pub stdin: &'a mut dyn BufRead,
    /// Whether standard input is a terminal, so a password may be prompted for.
    pub stdin_is_tty: bool,
    /// The clock, in Unix milliseconds.
    pub now_ms: fn() -> u64,
    /// Asks for the master password without echo.
    pub prompt_password: &'a dyn Fn() -> Option<String>,
    /// How long to wait for the lock.
    pub lock_wait: Duration,
}

/// A failure, with its exit code, a stable code for `--json`, and candidates when several entries matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliError {
    exit: u8,
    code: &'static str,
    message: String,
    candidates: Vec<String>,
}

impl CliError {
    pub(crate) fn new(exit: u8, code: &'static str, message: &str) -> Self {
        Self {
            exit,
            code,
            message: message.to_owned(),
            candidates: Vec::new(),
        }
    }

    pub(crate) fn with_candidates(
        exit: u8,
        code: &'static str,
        message: &str,
        candidates: Vec<String>,
    ) -> Self {
        Self {
            exit,
            code,
            message: message.to_owned(),
            candidates,
        }
    }
}

/// Runs the command line with `args` (without the program name) and returns the exit code.
///
/// Results go to `out`, errors to `err` (with `--json`, both go to `out` as JSON). Write failures are ignored:
/// there is nowhere left to report them.
pub fn run(
    args: &[String],
    context: &mut Context<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> u8 {
    let command = match parse(args) {
        Ok(command) => command,
        Err(UsageError(message)) => {
            let _ = writeln!(err, "{NAME}: {message}\nRun 'otpkeep help' for usage.");
            return EXIT_USAGE;
        }
    };
    let (options, result) = match command {
        Command::Help => {
            let _ = out.write_all(HELP.as_bytes());
            return EXIT_OK;
        }
        Command::Version => {
            let _ = writeln!(out, "{NAME} {VERSION}");
            return EXIT_OK;
        }
        Command::Code {
            query,
            next,
            options,
        } => (
            options,
            open::open_service(context, options)
                .and_then(|mut service| commands::code(&mut service, &query, next)),
        ),
        Command::List { tag, options } => (
            options,
            open::open_service(context, options)
                .map(|service| commands::list(&service, tag.as_deref())),
        ),
    };
    report(options, result, out, err)
}

fn report(
    options: Options,
    result: Result<commands::Printed, CliError>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> u8 {
    match (result, options.json) {
        (Ok(printed), false) => {
            let _ = out.write_all(printed.text.as_bytes());
            EXIT_OK
        }
        (Ok(printed), true) => {
            let _ = writeln!(out, "{}", json!({ "ok": true, "data": printed.json }));
            EXIT_OK
        }
        (Err(error), false) => {
            let _ = writeln!(err, "{NAME}: {}", error.message);
            for candidate in &error.candidates {
                let _ = writeln!(err, "  {candidate}");
            }
            error.exit
        }
        (Err(error), true) => {
            let mut detail = json!({ "code": error.code, "message": error.message });
            if !error.candidates.is_empty() {
                detail["candidates"] = Value::from(error.candidates.clone());
            }
            let _ = writeln!(out, "{}", json!({ "ok": false, "error": detail }));
            error.exit
        }
    }
}

#[cfg(test)]
mod tests;

//! The `otpkeep` command line (design/07-platform.md §2.6, exit codes in design/10-reference.md §4.2).
//!
//! When the app is not running, the command line opens the vault itself, taking the single-writer lock.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use keep_platform::Keystore;
use keep_vault::sealed::KdfParams;
use serde_json::{Value, json};

mod args;
mod ask;
mod commands;
mod export;
mod files;
mod import;
mod init;
pub mod lan;
mod open;
mod sources;

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
/// A pairing or sync did not complete; the JSON error code says why.
pub const EXIT_NOT_SYNCED: u8 = 11;
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
  init           Create an empty vault whose key is kept in the system keychain
                   --master-password protect it with a master password instead
  import <file>  Add the accounts of <file>: an OTPKeep backup, a GoldenPassport export (.secrets or
                 accounts.json), a list of otpauth:// or otpauth-migration:// links, or a QR code image.
                 Accounts already in the vault are left out; nothing is changed or removed.
                   --yes                       do not ask before adding
                   --backup-password-file <f>  read the backup password from the first line of <f>
  export         Write every entry (not the trash) to a new file
                   --format backup             an encrypted OTPKeep backup (asks for a backup password)
                   --format uris               a list of otpauth:// links
                   --format goldenpassport     a .secrets file GoldenPassport can import
                   --format google             Google Authenticator transfer codes (a folder of SVG images)
                   --out <path>                the file (or folder, for google) to create; never overwritten
                   --plaintext                 required for every format except backup: the file is unencrypted
                   --yes                       do not ask before writing an unencrypted file
                   --backup-password-file <f>  read the backup password from the first line of <f>
  sync wait      Open a sync window: show a code, then allow the member that enters it
                   --minutes <1-5>  how long to wait (default 2)
  sync join      Find this group's waiting device and sync with it, entering the code it shows
                   --to <name|fingerprint>  which device, when several wait
  pair open      Add a device to this device's sync group (starting a group if there is none)
                   --minutes <1-5>, --name <name of this device>, --qr  also print the invite and its QR code
  pair join      Join a sync group: find the waiting device and enter its pairing code
                   --invite <text>  use an invite instead of searching; --to <name|fingerprint>
                   --name <name of this device>; --replace  drop this device's entries (backed up first)
  devices        List the devices of the sync group
  devices remove <fingerprint>   Remove a device from the sync group for good
  group leave    Leave the sync group, keeping the entries
  rename <name>  Rename this device in the sync group
  version        Print the version
  help           Print this help

Options for every vault command:
  --json             Print {\"ok\": …, \"data\" | \"error\": …}
  --password-stdin   Read the master password from the first line of standard input
";

/// What the command line may ask the person at the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// The master password of an existing vault.
    MasterPassword,
    /// A master password for a new vault; asked twice.
    NewMasterPassword,
    /// The password of a backup being read.
    BackupPassword,
    /// A password for a new backup; asked twice.
    NewBackupPassword,
    /// A yes-or-no question; "y" or "yes" is yes.
    Confirm(String),
    /// A line of visible text, such as a code shown on another device.
    Text(String),
}

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
    /// Asks the person at the terminal; passwords without echo. `None` when nothing was answered.
    pub prompt: &'a dyn Fn(Prompt) -> Option<String>,
    /// New key-derivation parameters with a fresh salt (normally `keep_vault::master::new_kdf_params`).
    pub new_kdf: fn() -> Option<KdfParams>,
    /// How long to wait for the lock.
    pub lock_wait: Duration,
    /// Where pairing and sync run.
    pub network: lan::Network,
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
        Command::Init {
            master_password,
            options,
        } => (options, init::init(context, master_password, options)),
        Command::Import {
            path,
            transfer,
            options,
        } => (options, import::import(context, &path, &transfer, options)),
        Command::Export {
            format,
            out,
            transfer,
            options,
        } => (
            options,
            export::export(context, format, &out, &transfer, options),
        ),
        Command::Lan { action, options } => (options, run_lan(context, action, options, out)),
    };
    report(options, result, out, err)
}

fn run_lan(
    context: &mut Context<'_>,
    action: args::LanAction,
    options: Options,
    out: &mut impl Write,
) -> Result<commands::Printed, CliError> {
    use args::LanAction as A;
    match action {
        A::SyncWait { minutes } => lan::sync_wait(context, minutes, options, out),
        A::SyncJoin { to } => lan::sync_join(context, to.as_deref(), options, out),
        A::PairOpen { minutes, name, qr } => {
            lan::pair_open(context, minutes, name.as_deref(), qr, options, out)
        }
        A::PairJoin {
            invite,
            to,
            name,
            replace,
        } => lan::pair_join(
            context,
            invite.as_deref(),
            to.as_deref(),
            name.as_deref(),
            replace,
            options,
            out,
        ),
        A::Devices => lan::devices(context, options),
        A::RemoveDevice { fingerprint } => lan::remove_device(context, &fingerprint, options),
        A::Leave => lan::leave(context, options),
        A::Rename { name } => lan::rename(context, &name, options),
    }
}

fn report(
    options: Options,
    result: Result<commands::Printed, CliError>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> u8 {
    if let Ok(printed) = &result {
        for note in &printed.notes {
            let _ = writeln!(err, "{NAME}: {note}");
        }
    }
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

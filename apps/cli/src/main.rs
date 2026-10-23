//! Entry point of the `otpkeep` binary.

use std::io::{IsTerminal, Write, stderr, stdin, stdout};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use keep_platform::{Keystore, MemoryKeystore, SystemKeystore, data_dir};
use keep_vault::master::new_kdf_params;
use keep_vault::sealed::KdfParams;
use otpkeep_cli::{Context, LOCK_WAIT, Prompt, run};

/// Set to `memory` to use an empty in-memory keystore instead of the system one (for tests).
const KEYSTORE_OVERRIDE: &str = "OTPKEEP_KEYSTORE";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn new_kdf() -> Option<KdfParams> {
    new_kdf_params().ok()
}

/// Answers a prompt at the terminal: passwords without echo, new ones twice.
fn ask(prompt: Prompt) -> Option<String> {
    match prompt {
        Prompt::MasterPassword => rpassword::prompt_password("Master password: ").ok(),
        Prompt::BackupPassword => rpassword::prompt_password("Backup password: ").ok(),
        Prompt::NewMasterPassword => new_password("New master password: "),
        Prompt::NewBackupPassword => new_password("New backup password: "),
        Prompt::Confirm(question) => answer(&question),
    }
}

fn new_password(label: &str) -> Option<String> {
    let first = rpassword::prompt_password(label).ok()?;
    let again = rpassword::prompt_password("Again: ").ok()?;
    if first == again {
        return Some(first);
    }
    let _ = writeln!(stderr(), "otpkeep: the passwords do not match");
    None
}

/// Reads a line from the terminal itself, since standard input may be in use.
fn answer(question: &str) -> Option<String> {
    let _ = write!(stderr(), "{question} [y/N] ");
    let _ = stderr().flush();
    #[cfg(unix)]
    {
        use std::io::BufRead;
        let tty = std::fs::File::open("/dev/tty").ok()?;
        let mut line = String::new();
        std::io::BufReader::new(tty).read_line(&mut line).ok()?;
        Some(line)
    }
    #[cfg(not(unix))]
    {
        rpassword::read_password().ok()
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let memory = MemoryKeystore::new();
    let system = SystemKeystore::new();
    let keystore: &dyn Keystore = if std::env::var(KEYSTORE_OVERRIDE).as_deref() == Ok("memory") {
        &memory
    } else {
        &system
    };
    let input = stdin();
    let stdin_is_tty = input.is_terminal();
    let mut locked = input.lock();
    let mut context = Context {
        data_dir: data_dir(),
        keystore,
        stdin: &mut locked,
        stdin_is_tty,
        now_ms,
        prompt: &ask,
        new_kdf,
        lock_wait: LOCK_WAIT,
    };
    ExitCode::from(run(
        &args,
        &mut context,
        &mut stdout().lock(),
        &mut stderr().lock(),
    ))
}

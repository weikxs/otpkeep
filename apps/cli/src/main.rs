//! Entry point of the `otpkeep` binary.

use std::io::{IsTerminal, stderr, stdin, stdout};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use keep_platform::{Keystore, MemoryKeystore, SystemKeystore, data_dir};
use otpkeep_cli::{Context, LOCK_WAIT, run};

/// Set to `memory` to use an empty in-memory keystore instead of the system one (for tests).
const KEYSTORE_OVERRIDE: &str = "OTPKEEP_KEYSTORE";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
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
    let prompt = || rpassword::prompt_password("Master password: ").ok();
    let mut context = Context {
        data_dir: data_dir(),
        keystore,
        stdin: &mut locked,
        stdin_is_tty,
        now_ms,
        prompt_password: &prompt,
        lock_wait: LOCK_WAIT,
    };
    ExitCode::from(run(
        &args,
        &mut context,
        &mut stdout().lock(),
        &mut stderr().lock(),
    ))
}

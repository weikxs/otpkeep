//! Opening the vault from the command line when the app is not running (design/07-platform.md §2.6).

use keep_vault::open::{Startup, StopReason, UnlockError, start, unlock_with_password};
use keep_vault::service::VaultService;
use keep_vault::store::VaultPaths;
use zeroize::Zeroizing;

use crate::args::Options;
use crate::{CliError, Context, EXIT_AUTH_FAILED, EXIT_BUSY, EXIT_VAULT_ERROR, Prompt};

/// The vault, ready for reading and changing.
pub(crate) fn open_service(
    context: &mut Context<'_>,
    options: Options,
) -> Result<VaultService, CliError> {
    let paths = vault_paths(context)?;
    let now = context.now_ms;
    match start(&paths, context.keystore, context.lock_wait) {
        Startup::Ready(open) | Startup::ReadOnly { open, .. } => Ok(VaultService::new(open, now)),
        Startup::Fresh { .. } => Err(CliError::new(
            EXIT_VAULT_ERROR,
            "VAULT_ERROR",
            "there is no vault yet; create one with 'otpkeep init' or the OTPKeep app",
        )),
        Startup::NeedsMasterPassword(_) => {
            let password = master_password(context, options)?;
            let open =
                unlock_with_password(&paths, &password, context.lock_wait).map_err(|error| {
                    match error {
                        UnlockError::WrongPassword => CliError::new(
                            EXIT_AUTH_FAILED,
                            "AUTH_FAILED",
                            "the master password is wrong, or the vault is damaged",
                        ),
                        UnlockError::Stopped(reason) => stopped(reason),
                    }
                })?;
            Ok(VaultService::new(open, now))
        }
        Startup::Stopped(reason) => Err(stopped(reason)),
    }
}

/// Where the vault lives.
pub(crate) fn vault_paths(context: &Context<'_>) -> Result<VaultPaths, CliError> {
    let dir = context.data_dir.as_deref().ok_or_else(|| {
        CliError::new(
            EXIT_VAULT_ERROR,
            "VAULT_ERROR",
            "cannot find the data directory",
        )
    })?;
    Ok(VaultPaths::new(dir))
}

fn master_password(
    context: &mut Context<'_>,
    options: Options,
) -> Result<Zeroizing<String>, CliError> {
    read_password(
        context,
        options,
        Prompt::MasterPassword,
        "this vault has a master password; run in a terminal, or pass it with --password-stdin",
    )
}

/// A password from the first line of standard input with `--password-stdin`, or asked at the terminal.
pub(crate) fn read_password(
    context: &mut Context<'_>,
    options: Options,
    prompt: Prompt,
    unavailable: &str,
) -> Result<Zeroizing<String>, CliError> {
    if options.password_stdin {
        let mut line = Zeroizing::new(String::new());
        context.stdin.read_line(&mut line).map_err(|_| {
            CliError::new(
                EXIT_AUTH_FAILED,
                "AUTH_FAILED",
                "could not read the password from standard input",
            )
        })?;
        let trimmed = line.trim_end_matches(['\n', '\r']);
        return Ok(Zeroizing::new(trimmed.to_owned()));
    }
    if context.stdin_is_tty
        && let Some(password) = (context.prompt)(prompt)
    {
        return Ok(Zeroizing::new(password));
    }
    Err(CliError::new(EXIT_AUTH_FAILED, "AUTH_FAILED", unavailable))
}

fn stopped(reason: StopReason) -> CliError {
    let message = match reason {
        StopReason::Busy => {
            return CliError::new(
                EXIT_BUSY,
                "BUSY",
                "another OTPKeep process is using the data",
            );
        }
        StopReason::DataMissing => "the vault file is missing; open the OTPKeep app to restore it",
        StopReason::KeyMissing => {
            "the key for this vault is not on this device; open the OTPKeep app to restore"
        }
        StopReason::Damaged { .. } => "the vault is damaged; open the OTPKeep app to restore it",
        StopReason::NewerFormat(_) => "the vault was written by a newer OTPKeep; please update",
        StopReason::KeystoreUnavailable => "the system keystore is unavailable",
        StopReason::Io => "the data directory cannot be read",
    };
    CliError::new(EXIT_VAULT_ERROR, "VAULT_ERROR", message)
}

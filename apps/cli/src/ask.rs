//! Confirmations and backup passwords.

use std::path::Path;

use zeroize::Zeroizing;

use crate::{CliError, Context, EXIT_AUTH_FAILED, EXIT_CANCELLED, EXIT_INVALID_INPUT, Prompt};

/// Goes on when `yes` was given or the person at the terminal agrees.
pub(crate) fn confirm(context: &Context<'_>, yes: bool, question: &str) -> Result<(), CliError> {
    if yes {
        return Ok(());
    }
    if !context.stdin_is_tty {
        return Err(CliError::new(
            EXIT_CANCELLED,
            "CANCELLED",
            &format!("{question} Not confirmed: run in a terminal, or pass --yes"),
        ));
    }
    let answer = (context.prompt)(Prompt::Confirm(question.to_owned())).unwrap_or_default();
    if matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        return Ok(());
    }
    Err(CliError::new(
        EXIT_CANCELLED,
        "CANCELLED",
        "cancelled; nothing was changed",
    ))
}

/// The backup password: the first line of `file`, or asked at the terminal (twice when `new`).
pub(crate) fn backup_password(
    context: &Context<'_>,
    file: Option<&Path>,
    new: bool,
) -> Result<Zeroizing<String>, CliError> {
    if let Some(file) = file {
        let text = Zeroizing::new(std::fs::read_to_string(file).map_err(|_| {
            CliError::new(
                EXIT_INVALID_INPUT,
                "INVALID_INPUT",
                "cannot read the backup password file",
            )
        })?);
        let line = text.lines().next().unwrap_or_default();
        return Ok(Zeroizing::new(line.to_owned()));
    }
    let prompt = if new {
        Prompt::NewBackupPassword
    } else {
        Prompt::BackupPassword
    };
    if context.stdin_is_tty
        && let Some(password) = (context.prompt)(prompt)
    {
        return Ok(Zeroizing::new(password));
    }
    Err(CliError::new(
        EXIT_AUTH_FAILED,
        "AUTH_FAILED",
        "a backup password is needed; run in a terminal, or pass --backup-password-file",
    ))
}

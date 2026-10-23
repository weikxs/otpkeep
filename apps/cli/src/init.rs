//! `otpkeep init`: creating an empty vault.

use keep_vault::open::{CreateError, create_vault, create_with_master_password};
use serde_json::json;

use crate::args::Options;
use crate::commands::Printed;
use crate::open::{read_password, vault_paths};
use crate::{CliError, Context, EXIT_BUSY, EXIT_INVALID_INPUT, EXIT_VAULT_ERROR, Prompt};

/// Creates a vault whose key is kept in the keystore, or protected by a master password.
pub(crate) fn init(
    context: &mut Context<'_>,
    master_password: bool,
    options: Options,
) -> Result<Printed, CliError> {
    let paths = vault_paths(context)?;
    if let Some(dir) = context.data_dir.as_deref() {
        std::fs::create_dir_all(dir).map_err(|_| {
            CliError::new(
                EXIT_VAULT_ERROR,
                "VAULT_ERROR",
                "cannot create the data directory",
            )
        })?;
    }
    let created = if master_password {
        let password = read_password(
            context,
            options,
            Prompt::NewMasterPassword,
            "a master password is needed; run in a terminal, or pass it with --password-stdin",
        )?;
        let params = (context.new_kdf)().ok_or_else(|| {
            CliError::new(
                EXIT_VAULT_ERROR,
                "VAULT_ERROR",
                "the system random number generator is unavailable",
            )
        })?;
        create_with_master_password(&paths, &password, params, context.lock_wait)
    } else {
        create_vault(&paths, context.keystore, context.lock_wait)
    };
    created.map_err(create_error)?;
    let protection = if master_password {
        "master-password"
    } else {
        "keystore"
    };
    Ok(Printed {
        text: "Created an empty vault.\n".to_owned(),
        json: json!({ "created": true, "protection": protection }),
        notes: Vec::new(),
    })
}

fn create_error(error: CreateError) -> CliError {
    match error {
        CreateError::AlreadyExists => CliError::new(
            EXIT_VAULT_ERROR,
            "VAULT_ERROR",
            "a vault already exists here; nothing was changed",
        ),
        CreateError::Busy => CliError::new(
            EXIT_BUSY,
            "BUSY",
            "another OTPKeep process is using the data",
        ),
        CreateError::Keystore => CliError::new(
            EXIT_VAULT_ERROR,
            "VAULT_ERROR",
            "the system keystore cannot hold the key; use 'otpkeep init --master-password' instead",
        ),
        CreateError::Password => CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "the master password cannot be empty",
        ),
        CreateError::Io => CliError::new(
            EXIT_VAULT_ERROR,
            "VAULT_ERROR",
            "the new vault could not be written",
        ),
    }
}

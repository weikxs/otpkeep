//! Creating a vault, unlocking a master-password vault, and restoring the previous version.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keep_platform::Keystore;

use super::{
    KEYSTORE_ACCOUNT, OpenVault, Startup, StopReason, checked_header, open_with_key, stored_key,
};
use crate::master::{MasterError, derive_data_key};
use crate::model::{DeviceTag, Extra, Hlc, VaultDoc};
use crate::sealed::{DataKey, FileKind, KdfParams, KeySource, SealedHeader};
use crate::store::{LockError, VaultLock, VaultPaths, replace_vault};

/// Why a new vault was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CreateError {
    /// Data or a key already exists; creating would replace it.
    #[error("OTPKeep data already exists here")]
    AlreadyExists,
    /// Another OTPKeep process is using the data.
    #[error("another OTPKeep process is using the data")]
    Busy,
    /// The key could not be stored in the system keystore.
    #[error("the system keystore cannot store the key")]
    Keystore,
    /// The master password is empty or the parameters are invalid.
    #[error("the master password cannot be used")]
    Password,
    /// Random numbers, encryption or writing failed.
    #[error("the new vault could not be written")]
    Io,
}

/// Why a master-password vault was not unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnlockError {
    /// The password is wrong, or the file is damaged: encryption cannot tell the two apart.
    #[error("the master password is wrong, or the file is damaged")]
    WrongPassword,
    /// The vault cannot be opened for another reason.
    #[error("the vault cannot be opened")]
    Stopped(StopReason),
}

/// Creates an empty vault whose data key is kept in the system keystore.
///
/// # Errors
///
/// Returns [`CreateError`]; on failure no key is left behind in the keystore.
pub fn create_vault(
    paths: &VaultPaths,
    keystore: &dyn Keystore,
    lock_wait: Duration,
) -> Result<OpenVault, CreateError> {
    let lock = lock_for_creation(paths, lock_wait)?;
    if keystore
        .load(KEYSTORE_ACCOUNT)
        .map_err(|_| CreateError::Keystore)?
        .is_some()
    {
        return Err(CreateError::AlreadyExists);
    }
    let key = DataKey::generate().map_err(|_| CreateError::Io)?;
    let key_id = random_array()?;
    keystore
        .store(KEYSTORE_ACCOUNT, &stored_key(key_id, &key))
        .map_err(|_| CreateError::Keystore)?;
    let header = SealedHeader::new(FileKind::Vault, KeySource::Keystore, key_id);
    write_new(paths, header, key, lock).inspect_err(|_| {
        // Leave no key without a vault: that state would stop the next start as "data missing".
        let _ = keystore.delete(KEYSTORE_ACCOUNT);
    })
}

/// Creates an empty vault whose data key is derived from `password` with `params`.
///
/// # Errors
///
/// Returns [`CreateError`].
pub fn create_with_master_password(
    paths: &VaultPaths,
    password: &str,
    params: KdfParams,
    lock_wait: Duration,
) -> Result<OpenVault, CreateError> {
    let lock = lock_for_creation(paths, lock_wait)?;
    let key = derive_data_key(password, &params).map_err(|_| CreateError::Password)?;
    let header = SealedHeader {
        kdf: Some(params),
        ..SealedHeader::new(FileKind::Vault, KeySource::MasterPassword, random_array()?)
    };
    write_new(paths, header, key, lock)
}

/// Opens a master-password vault.
///
/// # Errors
///
/// Returns [`UnlockError`].
pub fn unlock_with_password(
    paths: &VaultPaths,
    password: &str,
    lock_wait: Duration,
) -> Result<OpenVault, UnlockError> {
    let lock = VaultLock::acquire(paths, lock_wait).map_err(|error| match error {
        LockError::Busy => UnlockError::Stopped(StopReason::Busy),
        LockError::Io => UnlockError::Stopped(StopReason::Io),
    })?;
    let bytes = fs::read(paths.vault()).map_err(|_| UnlockError::Stopped(StopReason::Io))?;
    let header = checked_header(paths, &bytes).map_err(UnlockError::Stopped)?;
    let damaged = StopReason::Damaged {
        previous_available: paths.prev().exists(),
    };
    let kdf = header
        .kdf
        .filter(|_| header.key_source == KeySource::MasterPassword);
    let key =
        derive_data_key(password, &kdf.ok_or(UnlockError::Stopped(damaged))?).map_err(|error| {
            match error {
                MasterError::EmptyPassword => UnlockError::WrongPassword,
                MasterError::BadParams => UnlockError::Stopped(damaged),
            }
        })?;
    match open_with_key(paths, &bytes, key, lock) {
        Ok(Startup::Ready(open) | Startup::ReadOnly { open, .. }) => Ok(open),
        Ok(_) => Err(UnlockError::Stopped(damaged)),
        Err(StopReason::Damaged { .. }) => Err(UnlockError::WrongPassword),
        Err(reason) => Err(UnlockError::Stopped(reason)),
    }
}

/// Puts `vault.okv.prev` back as the vault. The current file, if any, is kept as `vault.okv.damaged-<time>`.
///
/// # Errors
///
/// Returns the I/O error.
pub fn restore_previous(paths: &VaultPaths) -> std::io::Result<()> {
    let previous = fs::read(paths.prev())?;
    if paths.vault().exists() {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut kept = paths.vault().into_os_string();
        kept.push(format!(".damaged-{seconds}"));
        fs::rename(paths.vault(), kept)?;
    }
    replace_vault(paths, &previous)?;
    tracing::info!("previous vault version restored");
    Ok(())
}

fn lock_for_creation(paths: &VaultPaths, lock_wait: Duration) -> Result<VaultLock, CreateError> {
    let lock = VaultLock::acquire(paths, lock_wait).map_err(|error| match error {
        LockError::Busy => CreateError::Busy,
        LockError::Io => CreateError::Io,
    })?;
    if paths.vault().exists() || paths.prev().exists() {
        return Err(CreateError::AlreadyExists);
    }
    Ok(lock)
}

fn write_new(
    paths: &VaultPaths,
    header: SealedHeader,
    key: DataKey,
    lock: VaultLock,
) -> Result<OpenVault, CreateError> {
    let doc = VaultDoc {
        vault_id: random_array()?,
        entries: Vec::new(),
        tombstones: Vec::new(),
        group: None,
        device: None,
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag(random_array()?),
        extra: Extra::new(),
    };
    let mut open = OpenVault {
        paths: paths.clone(),
        header,
        key,
        doc: doc.clone(),
        read_only: false,
        _lock: lock,
    };
    open.save(doc).map_err(|_| CreateError::Io)?;
    Ok(open)
}

fn random_array<const N: usize>() -> Result<[u8; N], CreateError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| CreateError::Io)?;
    Ok(bytes)
}

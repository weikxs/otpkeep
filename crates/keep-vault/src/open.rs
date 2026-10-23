//! Opening the vault at startup: every combination of file and key leads to a defined outcome
//! (design/02-data.md §3.4, PRD FR-9.6 and FR-9.8). The rule is to stop and explain rather than start over:
//! nothing here ever replaces existing data with an empty vault.

use std::fs;
use std::time::Duration;

use keep_platform::{Keystore, KeystoreError};
use zeroize::Zeroizing;

use crate::codec::{DecodeError, decode_vault, encode_vault};
use crate::model::VaultDoc;
use crate::sealed::{
    DataKey, KdfParams, KeySource, OpenError, SEALED_FORMAT, SealedHeader, open, read_header, seal,
};
use crate::store::{LockError, VaultLock, VaultPaths, replace_vault};

mod create;

pub use create::{
    CreateError, UnlockError, create_vault, create_with_master_password, restore_previous,
    unlock_with_password,
};

/// The keystore account that holds this device's data key: the vault's key id (16 bytes) followed by the key.
pub const KEYSTORE_ACCOUNT: &str = "vault-data-key";

const KEY_ID_LEN: usize = 16;
const DATA_KEY_LEN: usize = 32;

/// What startup found.
#[derive(Debug)]
pub enum Startup {
    /// No vault and no key: a first start. The caller may create a vault.
    Fresh {
        /// Whether a system keystore can hold the new key; if not, a master password is needed.
        keystore_available: bool,
    },
    /// Opened for reading and writing.
    Ready(OpenVault),
    /// Opened, but a newer version has taken over this data, so nothing may be written.
    ReadOnly {
        /// The vault, read-only.
        open: OpenVault,
        /// The newest version found beside it.
        newer_version: u32,
    },
    /// The vault is protected by a master password; call [`unlock_with_password`].
    NeedsMasterPassword(KdfParams),
    /// The vault cannot be opened; the user has to choose what to do.
    Stopped(StopReason),
}

/// Why startup stopped. The files are left exactly as they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The keystore has a data key but there is no vault file: the file was deleted or moved.
    DataMissing,
    /// There is a vault file but no matching key: another user, a reinstalled system or a new device.
    KeyMissing,
    /// The file is damaged or was changed.
    Damaged {
        /// Whether `vault.okv.prev` can be tried with [`restore_previous`].
        previous_available: bool,
    },
    /// Written by a newer version of OTPKeep.
    NewerFormat(u16),
    /// The file's key is in the system keystore, which cannot be used right now.
    KeystoreUnavailable,
    /// Another OTPKeep process is using the data.
    Busy,
    /// The data directory cannot be read.
    Io,
}

/// Why a save failed. The vault in memory and on disk is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    /// The vault was opened read-only.
    #[error("the data is read-only because a newer version of OTPKeep uses it")]
    ReadOnly,
    /// The file could not be written.
    #[error("the data could not be written")]
    Io,
    /// The data could not be encrypted.
    #[error("the data could not be encrypted")]
    Seal,
}

/// An open vault. Holds the lock, so no other process writes while it is open.
#[derive(Debug)]
pub struct OpenVault {
    paths: VaultPaths,
    header: SealedHeader,
    key: DataKey,
    doc: VaultDoc,
    read_only: bool,
    _lock: VaultLock,
}

impl OpenVault {
    /// The current contents.
    #[must_use]
    pub fn doc(&self) -> &VaultDoc {
        &self.doc
    }

    /// Writes `doc` and makes it the current contents. On failure nothing changes.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError`].
    pub fn save(&mut self, doc: VaultDoc) -> Result<(), SaveError> {
        if self.read_only {
            return Err(SaveError::ReadOnly);
        }
        let plaintext = encode_vault(&doc);
        let sealed = seal(&self.header, &self.key, &plaintext).map_err(|_| SaveError::Seal)?;
        replace_vault(&self.paths, &sealed).map_err(|_| SaveError::Io)?;
        self.doc = doc;
        Ok(())
    }
}

/// Decides what to do with the data in `paths`, taking the lock (waiting up to `lock_wait`).
#[must_use]
pub fn start(paths: &VaultPaths, keystore: &dyn Keystore, lock_wait: Duration) -> Startup {
    let lock = match VaultLock::acquire(paths, lock_wait) {
        Ok(lock) => lock,
        Err(LockError::Busy) => return Startup::Stopped(StopReason::Busy),
        Err(LockError::Io) => return Startup::Stopped(StopReason::Io),
    };
    if !paths.vault().exists() {
        return startup_without_file(paths, keystore);
    }
    let Ok(bytes) = fs::read(paths.vault()) else {
        return Startup::Stopped(StopReason::Io);
    };
    let header = match checked_header(paths, &bytes) {
        Ok(header) => header,
        Err(reason) => return Startup::Stopped(reason),
    };
    if header.key_source == KeySource::MasterPassword {
        return match header.kdf {
            Some(kdf) => Startup::NeedsMasterPassword(kdf),
            None => Startup::Stopped(damaged(paths)),
        };
    }
    let key = match keystore_key(keystore, header.key_id) {
        Ok(key) => key,
        Err(reason) => return Startup::Stopped(reason),
    };
    match open_with_key(paths, &bytes, key, lock) {
        Ok(opened) => opened,
        Err(reason) => Startup::Stopped(reason),
    }
}

fn startup_without_file(paths: &VaultPaths, keystore: &dyn Keystore) -> Startup {
    if paths.prev().exists() {
        return Startup::Stopped(StopReason::Damaged {
            previous_available: true,
        });
    }
    match keystore.load(KEYSTORE_ACCOUNT) {
        Ok(Some(_)) => Startup::Stopped(StopReason::DataMissing),
        Ok(None) => Startup::Fresh {
            keystore_available: true,
        },
        Err(_) => Startup::Fresh {
            keystore_available: false,
        },
    }
}

/// Reads the header and rejects formats this version cannot open.
pub(crate) fn checked_header(paths: &VaultPaths, bytes: &[u8]) -> Result<SealedHeader, StopReason> {
    let header = read_header(bytes).map_err(|_| damaged(paths))?;
    if header.format > SEALED_FORMAT {
        return Err(StopReason::NewerFormat(header.format));
    }
    if header.format < SEALED_FORMAT {
        return Err(damaged(paths));
    }
    Ok(header)
}

fn keystore_key(keystore: &dyn Keystore, key_id: [u8; KEY_ID_LEN]) -> Result<DataKey, StopReason> {
    let stored = match keystore.load(KEYSTORE_ACCOUNT) {
        Ok(Some(stored)) => stored,
        Ok(None) | Err(KeystoreError::NotFound) => return Err(StopReason::KeyMissing),
        Err(_) => return Err(StopReason::KeystoreUnavailable),
    };
    let (stored_id, key) = split_stored_key(&stored).ok_or(StopReason::KeyMissing)?;
    if stored_id != key_id {
        return Err(StopReason::KeyMissing);
    }
    Ok(key)
}

/// Splits the keystore value into key id and data key.
pub(crate) fn split_stored_key(stored: &[u8]) -> Option<([u8; KEY_ID_LEN], DataKey)> {
    if stored.len() != KEY_ID_LEN + DATA_KEY_LEN {
        return None;
    }
    let (id, key) = stored.split_at(KEY_ID_LEN);
    let mut key_bytes = Zeroizing::new([0u8; DATA_KEY_LEN]);
    key_bytes.copy_from_slice(key);
    Some((id.try_into().ok()?, DataKey::from_bytes(*key_bytes)))
}

/// The keystore value for a key id and data key.
pub(crate) fn stored_key(key_id: [u8; KEY_ID_LEN], key: &DataKey) -> Zeroizing<Vec<u8>> {
    let mut stored = Zeroizing::new(Vec::with_capacity(KEY_ID_LEN + DATA_KEY_LEN));
    stored.extend_from_slice(&key_id);
    stored.extend_from_slice(key.expose());
    stored
}

/// Decrypts and decodes the vault, and decides between read-write and read-only.
pub(crate) fn open_with_key(
    paths: &VaultPaths,
    bytes: &[u8],
    key: DataKey,
    lock: VaultLock,
) -> Result<Startup, StopReason> {
    let (header, plaintext) = open(bytes, &key).map_err(|error| match error {
        OpenError::UnsupportedFormat(format) => StopReason::NewerFormat(format),
        _ => damaged(paths),
    })?;
    let doc = decode_vault(&plaintext).map_err(|error| match error {
        DecodeError::UnsupportedFormat(format) => {
            StopReason::NewerFormat(u16::try_from(format).unwrap_or(u16::MAX))
        }
        _ => damaged(paths),
    })?;
    let newer = paths.newer_vault_versions().map_err(|_| StopReason::Io)?;
    let open = OpenVault {
        paths: paths.clone(),
        header,
        key,
        doc,
        read_only: !newer.is_empty(),
        _lock: lock,
    };
    Ok(match newer.last() {
        Some(&newer_version) => Startup::ReadOnly {
            open,
            newer_version,
        },
        None => Startup::Ready(open),
    })
}

fn damaged(paths: &VaultPaths) -> StopReason {
    StopReason::Damaged {
        previous_available: paths.prev().exists(),
    }
}

#[cfg(test)]
mod tests;

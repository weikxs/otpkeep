//! Password-protected backups (PRD FR-6.1, design/05-transfer.md §2).
//!
//! A backup is a sealed file of kind "backup" whose key comes from the backup password through Argon2id, the
//! same format as the vault. It holds the chosen entries only: no sync group, device identity, history or
//! settings. Reading it returns import candidates, so restoring goes through the import preview.

use keep_vault::master::{MasterError, derive_data_key};
use keep_vault::sealed::{
    FileKind, KdfParams, KeySource, OpenError, SEALED_FORMAT, SealedHeader, open, read_header, seal,
};
use keep_vault::{DeviceTag, Entry, Extra, Hlc, VaultDoc, decode_vault, encode_vault};

use crate::import::{Candidate, Origin};

/// Why a backup could not be written or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BackupError {
    /// The password is empty.
    #[error("the backup password is empty")]
    EmptyPassword,
    /// The file is not an OTPKeep backup.
    #[error("this file is not an OTPKeep backup")]
    NotABackup,
    /// The password is wrong, or the file was changed: encryption cannot tell the two apart.
    #[error("the password is wrong, or the backup was changed or damaged")]
    WrongPassword,
    /// The backup's readable part is damaged.
    #[error("the backup is damaged")]
    Damaged,
    /// Written by a newer version of OTPKeep.
    #[error("the backup was made by a newer version of OTPKeep")]
    NewerFormat,
    /// Random numbers or encryption failed.
    #[error("the backup could not be written")]
    Failed,
}

/// Writes a backup of `entries` protected by `password`, using `params` (normally
/// [`keep_vault::master::new_kdf_params`]).
///
/// # Errors
///
/// Returns [`BackupError`].
pub fn write_backup(
    entries: &[&Entry],
    password: &str,
    params: KdfParams,
) -> Result<Vec<u8>, BackupError> {
    let key = derive_data_key(password, &params).map_err(|error| match error {
        MasterError::EmptyPassword => BackupError::EmptyPassword,
        MasterError::BadParams => BackupError::Failed,
    })?;
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|_| BackupError::Failed)?;
    let doc = VaultDoc {
        vault_id: id,
        entries: entries.iter().map(|e| (*e).clone()).collect(),
        tombstones: Vec::new(),
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([0; 8]),
        extra: Extra::new(),
    };
    let header = SealedHeader {
        kdf: Some(params),
        ..SealedHeader::new(FileKind::Backup, KeySource::MasterPassword, id)
    };
    seal(&header, &key, &encode_vault(&doc)).map_err(|_| BackupError::Failed)
}

/// Reads a backup into import candidates.
///
/// # Errors
///
/// Returns [`BackupError`].
pub fn read_backup(bytes: &[u8], password: &str) -> Result<Vec<Candidate>, BackupError> {
    let header = read_header(bytes).map_err(|error| match error {
        OpenError::TooShort | OpenError::BadMagic => BackupError::NotABackup,
        _ => BackupError::Damaged,
    })?;
    if header.kind != FileKind::Backup || header.key_source != KeySource::MasterPassword {
        return Err(BackupError::NotABackup);
    }
    if header.format > SEALED_FORMAT {
        return Err(BackupError::NewerFormat);
    }
    let params = header.kdf.ok_or(BackupError::Damaged)?;
    let key = derive_data_key(password, &params).map_err(|error| match error {
        MasterError::EmptyPassword => BackupError::EmptyPassword,
        MasterError::BadParams => BackupError::Damaged,
    })?;
    let (_, plaintext) = open(bytes, &key).map_err(|error| match error {
        OpenError::AuthFailed => BackupError::WrongPassword,
        OpenError::UnsupportedFormat(_) => BackupError::NewerFormat,
        _ => BackupError::Damaged,
    })?;
    let doc = decode_vault(&plaintext).map_err(|_| BackupError::Damaged)?;
    Ok(doc
        .entries
        .iter()
        .enumerate()
        .map(|(index, e)| candidate(e, index))
        .collect())
}

fn candidate(entry: &Entry, index: usize) -> Candidate {
    Candidate {
        title: entry.title.value.clone(),
        issuer: entry.issuer.value.clone(),
        account: entry.account.value.clone(),
        secret: entry.key.value.secret.clone(),
        spec: entry.key.value.spec,
        counter: entry.key.value.counter,
        note: entry.note.value.clone(),
        tags: entry.tags.value.clone(),
        origin: Origin::Item(index),
    }
}

#[cfg(test)]
mod tests;

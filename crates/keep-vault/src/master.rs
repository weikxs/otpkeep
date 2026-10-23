//! Deriving the data key from a master password, for systems without a keystore (design/02-data.md §3.2).

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::sealed::{DataKey, KdfParams, SealError};

/// Argon2id memory cost for new master passwords: 64 MiB.
pub const KDF_MEMORY_KIB: u32 = 64 * 1024;
/// Argon2id iterations for new master passwords.
pub const KDF_TIME_COST: u32 = 3;
/// Argon2id parallelism for new master passwords.
pub const KDF_PARALLELISM: u32 = 1;

/// Upper bounds for parameters read from a file. The header is read before the file can be authenticated, so
/// a tampered file must not be able to make the app allocate or compute without bound.
const MAX_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_TIME_COST: u32 = 16;
const MAX_PARALLELISM: u32 = 8;

const KEY_LEN: usize = 32;

/// Why a key could not be derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MasterError {
    /// The master password is empty.
    #[error("the master password is empty")]
    EmptyPassword,
    /// The stored parameters are not valid Argon2id parameters.
    #[error("the key derivation parameters are invalid")]
    BadParams,
}

/// Parameters for a new master password: recommended costs and a new random salt.
///
/// # Errors
///
/// Returns [`SealError::RandomUnavailable`] when the system random source fails.
pub fn new_kdf_params() -> Result<KdfParams, SealError> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|_| SealError::RandomUnavailable)?;
    Ok(KdfParams {
        salt,
        m_cost_kib: KDF_MEMORY_KIB,
        t_cost: KDF_TIME_COST,
        p_cost: KDF_PARALLELISM,
    })
}

/// Derives the data key from `password` with Argon2id.
///
/// # Errors
///
/// - [`MasterError::EmptyPassword`] for an empty password.
/// - [`MasterError::BadParams`] when the parameters are not valid Argon2id parameters.
pub fn derive_data_key(password: &str, params: &KdfParams) -> Result<DataKey, MasterError> {
    if password.is_empty() {
        return Err(MasterError::EmptyPassword);
    }
    if params.m_cost_kib > MAX_MEMORY_KIB
        || params.t_cost > MAX_TIME_COST
        || params.p_cost > MAX_PARALLELISM
    {
        return Err(MasterError::BadParams);
    }
    let argon_params = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        Some(KEY_LEN),
    )
    .map_err(|_| MasterError::BadParams)?;
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params)
        .hash_password_into(password.as_bytes(), &params.salt, &mut *key)
        .map_err(|_| MasterError::BadParams)?;
    Ok(DataKey::from_bytes(*key))
}

#[cfg(test)]
mod tests;

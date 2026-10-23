//! The encrypted vault: entries, key material, storage, locking and migration.

mod codec;
mod model;
pub mod sealed;

pub use codec::{DecodeError, decode_vault, encode_vault};
pub use model::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial, Tombstone, VaultDoc};

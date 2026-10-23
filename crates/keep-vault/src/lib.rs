//! The encrypted vault: entries, key material, storage, locking and migration.

mod codec;
pub mod master;
mod model;
pub mod open;
pub mod rank;
pub mod sealed;
pub mod store;

pub use codec::{DecodeError, decode_vault, encode_vault};
pub use model::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial, Tombstone, VaultDoc};

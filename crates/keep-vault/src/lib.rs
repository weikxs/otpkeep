//! The encrypted vault: entries, key material, storage, locking and migration.

mod codec;
pub mod edit;
pub mod history;
pub mod master;
mod model;
pub mod open;
pub mod rank;
pub mod sealed;
pub mod service;
pub mod store;

pub use codec::{DecodeError, decode_vault, encode_vault};
pub use model::{
    DeviceId, DeviceSecrets, DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial,
    MemberEvent, MemberKind, Platform, SyncGroup, Tombstone, VaultDoc,
};

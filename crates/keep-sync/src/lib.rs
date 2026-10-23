//! LAN pairing and sync: device identity, membership and merging (design/06-sync.md).

pub mod digest;
pub mod identity;
pub mod membership;
pub mod merge;

#[cfg(test)]
mod testing;

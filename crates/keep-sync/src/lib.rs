//! LAN pairing and sync: device identity, membership and merging (design/06-sync.md).

pub mod digest;
pub mod housekeeping;
pub mod identity;
pub mod lan;
pub mod membership;
pub mod merge;
pub mod session;

#[cfg(test)]
mod testing;

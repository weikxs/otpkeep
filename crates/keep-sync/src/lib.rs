//! LAN pairing and sync: device identity, membership and merging (design/06-sync.md).

pub mod channel;
pub mod digest;
pub mod flow;
pub mod group;
pub mod housekeeping;
pub mod identity;
pub mod invite;
pub mod lan;
pub mod membership;
pub mod merge;
pub mod session;

#[cfg(test)]
mod testing;

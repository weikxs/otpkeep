//! Whole pairing and sync sessions over TCP: listening, the handshake, the people's decisions and the data
//! phase (design/06-sync.md §3–5). Nothing here writes the vault; the caller commits what a session returns.

pub mod listen;
pub mod peer;
pub mod sync_join;
pub mod sync_open;
mod window_loop;

#[cfg(test)]
mod tests;

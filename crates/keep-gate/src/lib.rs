//! Local entry points: the browser management page, the local HTTP API and the CLI channel.
//!
//! So far: the browser management page server (design/07-platform.md §2.5). The app supplies the data
//! through [`Backend`]; this crate owns the security model.

pub mod api;
pub mod server;
pub mod session;

pub use api::{Asset, Backend, EntryEdit, GateEntry, GateError, ShortcutChange};
pub use server::RunningGate;

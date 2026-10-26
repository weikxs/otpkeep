//! The accounts the menu bar menu lists: the pinned ones first, then the ones used most recently in this run,
//! then the rest in list order. Which codes were used is kept in memory only, never saved.

use keep_vault::Entry;

use super::{Core, PanelState, Vault, row};

/// The menu lists at most this many accounts; the search panel finds the others.
pub const MENU_LIMIT: usize = 15;
/// How many recently used entries are remembered.
const RECENT_LIMIT: usize = 50;

impl Core {
    /// What the menu bar menu shows: the state of the data and up to [`MENU_LIMIT`] accounts with their
    /// codes. `recent` lists entry ids, most recently used first.
    #[must_use]
    pub fn menu_state(&self, recent: &[String]) -> PanelState {
        let state = self.state("");
        let Vault::Open { service, .. } = &self.vault else {
            return state;
        };
        let ordered = service.ordered();
        let entries = pick(&ordered, recent, MENU_LIMIT)
            .into_iter()
            .map(|entry| row(service, entry, state.now_ms))
            .collect();
        PanelState { entries, ..state }
    }
}

/// Up to `limit` of `ordered` (the entries in list order): pinned first, then those in `recent` in its order,
/// then the others in list order.
fn pick<'a>(ordered: &[&'a Entry], recent: &[String], limit: usize) -> Vec<&'a Entry> {
    let unpinned: Vec<(String, &'a Entry)> = ordered
        .iter()
        .filter(|entry| !entry.pinned.value)
        .map(|entry| (entry.id.to_string(), *entry))
        .collect();
    let pinned = ordered.iter().copied().filter(|entry| entry.pinned.value);
    let used = recent.iter().filter_map(|id| {
        unpinned
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, entry)| *entry)
    });
    let rest = unpinned
        .iter()
        .filter(|(id, _)| !recent.contains(id))
        .map(|(_, entry)| *entry);
    pinned.chain(used).chain(rest).take(limit).collect()
}

/// `recent` after `id` was used: `id` first, each entry once, at most [`RECENT_LIMIT`] of them.
#[must_use]
pub fn with_used(recent: &[String], id: &str) -> Vec<String> {
    std::iter::once(id.to_owned())
        .chain(recent.iter().filter(|other| *other != id).cloned())
        .take(RECENT_LIMIT)
        .collect()
}

#[cfg(test)]
mod tests;

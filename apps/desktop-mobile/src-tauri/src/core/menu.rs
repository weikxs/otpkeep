//! The accounts the menu bar menu lists: the pinned ones first, then the ones used most recently on this device
//! (remembered in its settings, see [`crate::settings::Settings::recent_entries`]), then the rest in list order.

use keep_vault::Entry;

use super::{Core, PanelState, Vault, row};

/// The menu lists at most this many accounts; the search panel finds the others.
pub const MENU_LIMIT: usize = 15;

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

#[cfg(test)]
mod tests;

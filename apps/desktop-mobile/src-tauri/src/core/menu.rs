//! The accounts the menu bar menu lists: on its first level the recently used ones (as this device remembers
//! them, see [`crate::settings::Settings::recent_entries`]), filled up in list order; every other account in
//! a "More accounts" submenu, in list order, up to [`MENU_LIMIT`].

use std::collections::HashSet;

use keep_vault::Entry;

use super::{Core, PanelEntry, PanelState, Vault, row};

/// The "More accounts" submenu lists at most this many accounts, so a huge vault cannot make a menu the system
/// struggles to draw; the menu says how many were left out, and the search panel finds them.
pub const MENU_LIMIT: usize = 1000;

/// What the menu bar menu shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuView {
    /// The state of the data, with the accounts of the menu's first level.
    pub state: PanelState,
    /// The other accounts, in list order (at most [`MENU_LIMIT`]).
    pub more: Vec<PanelEntry>,
    /// How many accounts did not fit in the menu.
    pub left_out: usize,
}

impl Core {
    /// What the menu bar menu shows, with codes: `first_level` accounts, the ones in `recent` (entry ids, most
    /// recently used first; ids of entries that are gone are skipped) first, then the others.
    #[must_use]
    pub fn menu_view(&self, recent: &[String], first_level: usize) -> MenuView {
        let summary = self.state("");
        let Vault::Open { service, .. } = &self.vault else {
            return MenuView {
                state: PanelState {
                    entries: Vec::new(),
                    ..summary
                },
                more: Vec::new(),
                left_out: 0,
            };
        };
        let now_ms = summary.now_ms;
        let ordered = service.ordered();
        let (first, rest) = split(&ordered, recent, first_level);
        let to_row = |entry: &Entry| row(service, entry, now_ms);
        MenuView {
            state: PanelState {
                entries: first.into_iter().map(to_row).collect(),
                ..summary
            },
            more: rest
                .iter()
                .take(MENU_LIMIT)
                .map(|entry| to_row(entry))
                .collect(),
            left_out: rest.len().saturating_sub(MENU_LIMIT),
        }
    }
}

/// The first `first_level` entries (those in `recent` first, then in list order) and the rest in list order.
fn split<'a>(
    ordered: &[&'a Entry],
    recent: &[String],
    first_level: usize,
) -> (Vec<&'a Entry>, Vec<&'a Entry>) {
    let used = recent.iter().filter_map(|id| {
        ordered
            .iter()
            .copied()
            .find(|entry| entry.id.to_string() == *id)
    });
    let mut first: Vec<&Entry> = Vec::new();
    let mut taken = HashSet::new();
    for entry in used.chain(ordered.iter().copied()) {
        if first.len() == first_level {
            break;
        }
        if taken.insert(entry.id) {
            first.push(entry);
        }
    }
    let rest = ordered
        .iter()
        .copied()
        .filter(|entry| !taken.contains(&entry.id))
        .collect();
    (first, rest)
}

#[cfg(test)]
mod tests;

//! Which global shortcut copies which entry (PRD FR-5.2, design/10-reference.md §2).
//!
//! The first ten rows get `⌃⌥⌘1` … `⌃⌥⌘0` on macOS (no defaults on Windows and Linux); any entry can also
//! have its own shortcut, kept on this device only. This module holds the rules; registering with the system
//! happens in `hotkeys`.

use std::collections::BTreeMap;

use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

/// Rows with a default shortcut.
pub const DEFAULT_ROWS: usize = 10;
/// Kept free for the search panel (`⌃⌥⌘Space`, FR-3.8).
const SEARCH_PANEL: &str = "control+alt+super+Space";

/// What a shortcut copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The entry in this place of the list (0-based).
    Row(usize),
    /// This entry, wherever it is.
    Entry(String),
}

/// Who already uses a shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    /// The default shortcut of a row (0-based).
    Row(usize),
    /// Another entry's own shortcut.
    Entry(String),
    /// Kept for the search panel.
    SearchPanel,
}

/// Why a shortcut cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutError {
    /// Not a key combination, or one without Control, Alt / Option or Command / Windows.
    Invalid,
    /// Already in use.
    Taken(Holder),
}

/// Turns what the page recorded (`Control+Alt+Super+KeyG`) into the canonical form
/// (`control+alt+super+KeyG`). Shift alone is not enough: it would fire while typing.
///
/// # Errors
///
/// Returns [`ShortcutError::Invalid`] for anything else.
pub fn canonical(input: &str) -> Result<String, ShortcutError> {
    let shortcut: Shortcut = input.trim().parse().map_err(|_| ShortcutError::Invalid)?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err(ShortcutError::Invalid);
    }
    Ok(shortcut.into_string())
}

/// All shortcuts of this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutMap {
    defaults: bool,
    entries: BTreeMap<String, String>,
}

impl ShortcutMap {
    /// `defaults`: whether the first rows get default shortcuts (macOS).
    #[must_use]
    pub fn new(defaults: bool, entries: BTreeMap<String, String>) -> Self {
        Self { defaults, entries }
    }

    /// The default shortcut of row `index` (0-based), if it has one. An entry's own shortcut wins over a
    /// default it equals (it could be set while the defaults were off).
    #[must_use]
    pub fn default_for(&self, index: usize) -> Option<String> {
        if !self.defaults || index >= DEFAULT_ROWS {
            return None;
        }
        let digit = (index + 1) % DEFAULT_ROWS;
        let shortcut = format!("control+alt+super+Digit{digit}");
        (!self.entries.values().any(|own| *own == shortcut)).then_some(shortcut)
    }

    /// A copy with the row defaults turned on or off.
    #[must_use]
    pub fn with_defaults(&self, defaults: bool) -> Self {
        Self {
            defaults,
            entries: self.entries.clone(),
        }
    }

    /// The entry's own shortcut.
    #[must_use]
    pub fn own(&self, entry: &str) -> Option<&str> {
        self.entries.get(entry).map(String::as_str)
    }

    /// The entries' own shortcuts, entry id → shortcut.
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    /// Every shortcut to register, with what it copies.
    #[must_use]
    pub fn targets(&self) -> Vec<(String, Target)> {
        let defaults =
            (0..DEFAULT_ROWS).filter_map(|i| self.default_for(i).map(|s| (s, Target::Row(i))));
        let own = self
            .entries
            .iter()
            .map(|(entry, s)| (s.clone(), Target::Entry(entry.clone())));
        defaults.chain(own).collect()
    }

    /// Who uses `shortcut` (canonical), not counting `entry` itself.
    #[must_use]
    pub fn holder(&self, shortcut: &str, entry: &str) -> Option<Holder> {
        if shortcut == SEARCH_PANEL {
            return Some(Holder::SearchPanel);
        }
        if let Some(row) =
            (0..DEFAULT_ROWS).find(|&i| self.default_for(i).as_deref() == Some(shortcut))
        {
            return Some(Holder::Row(row));
        }
        self.entries
            .iter()
            .find(|(other, s)| other.as_str() != entry && s.as_str() == shortcut)
            .map(|(other, _)| Holder::Entry(other.clone()))
    }

    /// A copy with `entry`'s own shortcut set to `shortcut` (canonical), or removed with `None`.
    ///
    /// # Errors
    ///
    /// Returns [`ShortcutError::Taken`] when something else uses it.
    pub fn with(&self, entry: &str, shortcut: Option<&str>) -> Result<Self, ShortcutError> {
        let mut entries = self.entries.clone();
        match shortcut {
            Some(shortcut) => {
                if let Some(holder) = self.holder(shortcut, entry) {
                    return Err(ShortcutError::Taken(holder));
                }
                entries.insert(entry.to_owned(), shortcut.to_owned());
            }
            None => {
                entries.remove(entry);
            }
        }
        Ok(Self {
            defaults: self.defaults,
            entries,
        })
    }

    /// The shortcut an entry in row `index` shows: its own, else its row's default.
    #[must_use]
    pub fn shown_for(&self, entry: &str, index: usize) -> Option<String> {
        self.own(entry)
            .map(str::to_owned)
            .or_else(|| self.default_for(index))
    }
}

#[cfg(test)]
mod tests;

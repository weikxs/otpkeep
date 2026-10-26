//! `settings.json`: this device's settings, never synced (design/02-data.md §2). It holds no secrets and no
//! account names: an entry's own shortcut is stored under the entry's id only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";
/// How long a copied code stays on the clipboard when the setting says nothing (design/10-reference.md §2).
pub const DEFAULT_CLEAR_S: u16 = 30;
/// How many recently used entries the menu bar menu remembers.
pub const RECENT_LIMIT: usize = 10;

/// The settings this version knows; fields from newer versions are kept as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Entry id → its own global shortcut, in canonical form.
    #[serde(default)]
    pub entry_shortcuts: BTreeMap<String, String>,
    /// `zh` or `en`; absent follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `light` or `dark`; absent follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Seconds before a copied code is cleared; 0 never clears; absent is [`DEFAULT_CLEAR_S`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard_clear_s: Option<u16>,
    /// Whether the first ten rows get shortcuts; absent is the system's default (on for macOS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_shortcuts: Option<bool>,
    /// The search panel's shortcut; absent is the system's default (`⌃⌥⌘Space` on macOS), empty is off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_shortcut: Option<String>,
    /// Ids of the entries whose codes were used most recently on this device, newest first (at most
    /// [`RECENT_LIMIT`]). Ids only: no names, codes or secrets. Never synced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_entries: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Settings {
    /// How long a copied code stays on the clipboard; `None` when it is never cleared.
    #[must_use]
    pub fn clear_after(&self) -> Option<Duration> {
        match self.clipboard_clear_s.unwrap_or(DEFAULT_CLEAR_S) {
            0 => None,
            seconds => Some(Duration::from_secs(u64::from(seconds))),
        }
    }

    /// The settings after the code of `id` was used: `id` first, each entry once, only entries for which
    /// `exists` holds, at most [`RECENT_LIMIT`] of them.
    #[must_use]
    pub fn with_recent(&self, id: &str, exists: impl Fn(&str) -> bool) -> Self {
        let older = self
            .recent_entries
            .iter()
            .map(String::as_str)
            .filter(|entry| *entry != id);
        let recent_entries = std::iter::once(id)
            .chain(older)
            .filter(|entry| exists(entry))
            .take(RECENT_LIMIT)
            .map(str::to_owned)
            .collect();
        Self {
            recent_entries,
            ..self.clone()
        }
    }

    /// The search panel's shortcut in effect, if any.
    #[must_use]
    pub fn search_shortcut_on(&self) -> Option<String> {
        match &self.search_shortcut {
            None => {
                cfg!(target_os = "macos").then(|| crate::shortcuts::SEARCH_PANEL_DEFAULT.to_owned())
            }
            Some(chosen) if chosen.is_empty() => None,
            Some(chosen) => Some(chosen.clone()),
        }
    }

    /// Whether the first ten rows get shortcuts.
    #[must_use]
    pub fn row_shortcuts_on(&self) -> bool {
        self.row_shortcuts.unwrap_or(cfg!(target_os = "macos"))
    }
}

/// Why the settings could not be saved.
#[derive(Debug, thiserror::Error)]
#[error("the settings could not be saved")]
pub struct SaveError;

/// Where the settings live; without a data directory they are kept in memory only.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: Option<PathBuf>,
}

impl SettingsFile {
    /// The settings file in `dir`.
    #[must_use]
    pub fn new(dir: Option<&Path>) -> Self {
        Self {
            path: dir.map(|d| d.join(FILE)),
        }
    }

    /// Reads the settings. A missing file gives the defaults; an unreadable one too, after a warning, and it
    /// is set aside so the next save does not destroy it.
    #[must_use]
    pub fn load(&self) -> Settings {
        let Some(path) = &self.path else {
            return Settings::default();
        };
        let Ok(bytes) = std::fs::read(path) else {
            return Settings::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            tracing::warn!("settings.json cannot be read; starting from the defaults");
            let _ = std::fs::rename(path, path.with_extension("json.unreadable"));
            Settings::default()
        })
    }

    /// Writes the settings: a temporary file first, then renamed over the old one.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError`] when the file cannot be written; the old file is then unchanged.
    pub fn save(&self, settings: &Settings) -> Result<(), SaveError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(settings).map_err(|_| SaveError)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|_| SaveError)?;
        std::fs::rename(&temporary, path).map_err(|_| {
            let _ = std::fs::remove_file(&temporary);
            SaveError
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, SettingsFile};

    fn temp_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("app-settings-")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn starts_from_the_defaults_without_a_file() {
        let dir = temp_dir();

        assert_eq!(
            SettingsFile::new(Some(dir.path())).load(),
            Settings::default()
        );
    }

    #[test]
    fn keeps_shortcuts_and_fields_it_does_not_know() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"theme":"dark","future":1,"entryShortcuts":{"e1":"control+alt+super+KeyG"}}"#,
        )
        .unwrap();
        let file = SettingsFile::new(Some(dir.path()));

        let mut settings = file.load();
        settings
            .entry_shortcuts
            .insert("e2".to_owned(), "alt+super+KeyM".to_owned());
        file.save(&settings).unwrap();

        let again = file.load();
        assert_eq!(again.entry_shortcuts.len(), 2);
        assert_eq!(again.theme.as_deref(), Some("dark"));
        assert_eq!(again.extra["future"], 1);
    }

    #[test]
    fn keeps_the_retired_panel_opacity_for_older_versions() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("settings.json"), r#"{"panelOpacity":80}"#).unwrap();
        let file = SettingsFile::new(Some(dir.path()));

        file.save(&file.load()).unwrap();

        assert_eq!(file.load().extra["panelOpacity"], 80);
    }

    #[test]
    fn remembers_recently_used_entries_newest_first_once_each() {
        let all = |_: &str| true;
        let settings = Settings::default()
            .with_recent("a", all)
            .with_recent("b", all)
            .with_recent("a", all);

        assert_eq!(settings.recent_entries, ["a", "b"]);
    }

    #[test]
    fn remembers_at_most_ten_entries_and_drops_those_gone() {
        let many = (0..15).fold(Settings::default(), |s, n| {
            s.with_recent(&n.to_string(), |_| true)
        });
        assert_eq!(many.recent_entries.len(), 10);
        assert_eq!(many.recent_entries[0], "14");

        let pruned = many.with_recent("14", |id| id != "13" && id != "12");
        assert_eq!(pruned.recent_entries[..3], ["14", "11", "10"]);
    }

    #[test]
    fn keeps_recent_entries_in_the_file() {
        let dir = temp_dir();
        let file = SettingsFile::new(Some(dir.path()));

        file.save(&Settings::default().with_recent("e1", |_| true))
            .unwrap();

        let text = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(text.contains("\"recentEntries\""));
        assert_eq!(file.load().recent_entries, ["e1"]);
    }

    #[test]
    fn sets_an_unreadable_file_aside() {
        let dir = temp_dir();
        std::fs::write(dir.path().join("settings.json"), b"{ not json").unwrap();

        let settings = SettingsFile::new(Some(dir.path())).load();

        assert_eq!(settings, Settings::default());
        assert!(dir.path().join("settings.json.unreadable").exists());
    }

    #[test]
    fn the_search_panel_shortcut_is_the_default_until_chosen_or_turned_off() {
        let chosen = |stored: Option<&str>| Settings {
            search_shortcut: stored.map(str::to_owned),
            ..Settings::default()
        };

        assert_eq!(
            chosen(None).search_shortcut_on().is_some(),
            cfg!(target_os = "macos")
        );
        assert_eq!(chosen(Some("")).search_shortcut_on(), None);
        assert_eq!(
            chosen(Some("control+alt+KeyK"))
                .search_shortcut_on()
                .as_deref(),
            Some("control+alt+KeyK")
        );
    }

    #[test]
    fn an_empty_search_shortcut_survives_saving_as_off() {
        let dir = temp_dir();
        let file = SettingsFile::new(Some(dir.path()));
        let off = Settings {
            search_shortcut: Some(String::new()),
            ..Settings::default()
        };

        file.save(&off).unwrap();

        assert_eq!(file.load().search_shortcut_on(), None);
    }

    #[test]
    fn without_a_directory_keeps_settings_in_memory() {
        let file = SettingsFile::new(None);

        assert!(file.save(&Settings::default()).is_ok());
        assert_eq!(file.load(), Settings::default());
    }
}

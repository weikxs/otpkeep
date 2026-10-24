//! `settings.json`: this device's settings, never synced (design/02-data.md §2). It holds no secrets and no
//! account names: an entry's own shortcut is stored under the entry's id only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";

/// The settings this version knows; fields from newer versions are kept as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Entry id → its own global shortcut, in canonical form.
    #[serde(default)]
    pub entry_shortcuts: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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
            r#"{"theme":"dark","entryShortcuts":{"e1":"control+alt+super+KeyG"}}"#,
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
        assert_eq!(again.extra["theme"], "dark");
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
    fn without_a_directory_keeps_settings_in_memory() {
        let file = SettingsFile::new(None);

        assert!(file.save(&Settings::default()).is_ok());
        assert_eq!(file.load(), Settings::default());
    }
}

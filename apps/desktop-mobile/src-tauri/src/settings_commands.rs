//! The settings page (design/10-reference.md §2). A change is announced, so the panel follows the theme
//! and the language at once.

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use tauri::{AppHandle, Emitter, State};

use crate::commands::CommandError;
use crate::hotkeys::{Hotkeys, UpdateError, set_search_shortcut, update_settings};
use crate::key_commands::shortcut_error;
use crate::preferences::{SettingsPatch, SettingsView, view};

#[tauri::command]
pub fn get_settings(hotkeys: State<'_, Hotkeys>) -> SettingsView {
    view(&hotkeys.settings())
}

#[tauri::command]
pub fn set_settings(patch: SettingsPatch, app: AppHandle) -> Result<SettingsView, CommandError> {
    let settings = update_settings(&app, &patch).map_err(|error| match error {
        UpdateError::Invalid(rule) => CommandError::with("INVALID", rule.to_owned()),
        UpdateError::Save => CommandError::code("SAVE_FAILED"),
    })?;
    Ok(announce(&app, &settings))
}

/// Sets the search panel's global shortcut, or turns it off with `None`.
#[tauri::command]
pub fn set_search_panel_shortcut(
    shortcut: Option<String>,
    app: AppHandle,
    hotkeys: State<'_, Hotkeys>,
) -> Result<SettingsView, CommandError> {
    set_search_shortcut(&app, shortcut.as_deref()).map_err(|error| shortcut_error(&app, error))?;
    let shown = announce(&app, &hotkeys.settings());
    // Row badges may change when the search panel gives back a row's default.
    let _ = app.emit("vault-changed", ());
    Ok(shown)
}

fn announce(app: &AppHandle, settings: &crate::settings::Settings) -> SettingsView {
    crate::desktop::relabel_tray(app);
    let shown = view(settings);
    let _ = app.emit("settings-changed", shown.clone());
    shown
}

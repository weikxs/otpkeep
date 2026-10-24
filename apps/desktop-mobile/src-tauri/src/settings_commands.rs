//! The settings page (design/10-reference.md §2). A change is announced, so the panel follows the theme
//! and the language at once.

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use tauri::{AppHandle, Emitter, State};

use crate::commands::CommandError;
use crate::hotkeys::{Hotkeys, UpdateError, update_settings};
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
    let shown = view(&settings);
    let _ = app.emit("settings-changed", shown.clone());
    Ok(shown)
}

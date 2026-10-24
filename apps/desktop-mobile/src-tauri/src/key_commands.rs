//! The commands behind the editor's advanced part: the key's parameters, a new secret, the service and
//! account names and the entry's own shortcut (PRD FR-4.4, FR-5.2).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use tauri::{AppHandle, Emitter, State};

use crate::commands::{AppCore, CommandError};
use crate::core::{KeyForm, KeyInfo};
use crate::hotkeys::{SetError, set_entry_shortcut, taken_detail};

type Result<T> = std::result::Result<T, CommandError>;

#[tauri::command]
pub fn key_info(id: &str, vault: State<'_, AppCore>) -> Result<KeyInfo> {
    Ok(vault.get().key_info(id)?)
}

/// The code with these parameters right now; nothing is saved.
#[tauri::command]
pub fn preview_key(id: &str, form: KeyForm, vault: State<'_, AppCore>) -> Result<Option<String>> {
    Ok(vault.get().preview_key(id, &form)?)
}

#[tauri::command]
pub fn replace_key(
    id: &str,
    form: KeyForm,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<()> {
    vault.get().replace_key(id, &form)?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub fn rename_entry(
    id: &str,
    issuer: Option<String>,
    account: Option<String>,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<()> {
    vault.get().rename(id, issuer, account)?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Sets or removes the entry's own global shortcut; kept on this device only.
#[tauri::command]
pub fn set_shortcut(
    id: &str,
    shortcut: Option<String>,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<()> {
    // The entry must exist before a shortcut is given to it.
    vault.get().key_info(id)?;
    set_entry_shortcut(&app, id, shortcut.as_deref())
        .map_err(|error| shortcut_error(&app, error))?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// A shortcut that could not be set, as the windows show it.
pub(crate) fn shortcut_error(app: &AppHandle, error: SetError) -> CommandError {
    match error {
        SetError::Invalid => CommandError::code("SHORTCUT_INVALID"),
        SetError::Rejected => CommandError::code("SHORTCUT_REJECTED"),
        SetError::Save => CommandError::code("SAVE_FAILED"),
        SetError::Taken(holder) => CommandError::with("SHORTCUT_TAKEN", taken_detail(app, &holder)),
    }
}

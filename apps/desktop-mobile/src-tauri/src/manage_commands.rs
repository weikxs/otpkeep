//! The commands the main window uses to keep the list in order (PRD FR-3.1, FR-3.11, FR-4.1–4.3). Every
//! change is announced, so the panel and the management page follow.

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use tauri::{AppHandle, Emitter, State};

use crate::commands::{AppCore, CommandError};
use crate::core::{BatchAction, CoreError, History, MoveTo, TrashItem};

type Result<T> = std::result::Result<T, CommandError>;

fn announce(app: &AppHandle, result: std::result::Result<(), CoreError>) -> Result<()> {
    result?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub fn set_pinned(id: &str, pinned: bool, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    announce(&app, vault.get().set_pinned(id, pinned))
}

#[tauri::command]
pub fn set_tags(
    id: &str,
    tags: Vec<String>,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<()> {
    announce(&app, vault.get().set_tags(id, tags))
}

#[tauri::command]
pub fn move_entry(id: &str, to: MoveTo, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    announce(&app, vault.get().move_to(id, &to))
}

#[tauri::command]
pub fn trash_entry(id: &str, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    announce(&app, vault.get().trash(id))
}

#[tauri::command]
pub fn restore_entry(id: &str, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    announce(&app, vault.get().restore(id))
}

#[tauri::command]
pub fn purge_entry(id: &str, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    announce(&app, vault.get().purge(id))
}

#[tauri::command]
pub fn trash_list(vault: State<'_, AppCore>) -> Result<Vec<TrashItem>> {
    Ok(vault.get().trash_list()?)
}

#[tauri::command]
pub fn entry_history(id: &str, vault: State<'_, AppCore>) -> Result<History> {
    Ok(vault.get().history(id)?)
}

/// Applies one action to several entries at once; returns how many changed.
#[tauri::command]
pub fn batch_entries(
    ids: Vec<String>,
    action: BatchAction,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<usize> {
    let changed = vault.get().batch(&ids, &action)?;
    let _ = app.emit("vault-changed", ());
    Ok(changed)
}

//! The sync page's commands (design/06-sync.md §8). Sessions report through the `sync-view` event; saved
//! data through `vault-changed`.

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::{AppCore, CommandError};
use crate::core::sync::GroupView;
use crate::native::{LEAVE_GROUP, REMOVE_DEVICE, SHOW_INVITE, confirm};
use crate::sync::join::{Looking, PairWith};
use crate::sync::{FoundView, Notify, SyncError, SyncView, Syncer};

type Result<T> = std::result::Result<T, CommandError>;

impl From<SyncError> for CommandError {
    fn from(error: SyncError) -> Self {
        Self::code(error.0)
    }
}

/// Reports the sessions to the windows.
pub struct AppNotify(pub AppHandle);

impl Notify for AppNotify {
    fn view(&self, view: &SyncView) {
        let _ = self.0.emit("sync-view", view);
    }

    fn vault_changed(&self) {
        let _ = self.0.emit("vault-changed", ());
    }
}

/// The group and the session, for the sync page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub group: GroupView,
    pub sync: SyncView,
}

#[tauri::command]
pub fn sync_state(vault: State<'_, AppCore>, syncer: State<'_, Syncer>) -> Result<SyncState> {
    let group = vault.get().group_view()?;
    Ok(SyncState {
        group,
        sync: syncer.view(),
    })
}

#[tauri::command]
pub async fn open_sync_window(minutes: Option<u64>, syncer: State<'_, Syncer>) -> Result<()> {
    Ok(syncer.open_sync_window(minutes).await?)
}

#[tauri::command]
pub async fn open_pair_window(
    name: Option<String>,
    minutes: Option<u64>,
    syncer: State<'_, Syncer>,
) -> Result<()> {
    Ok(syncer.open_pair_window(name.as_deref(), minutes).await?)
}

/// The pairing window's invite QR code, after the person confirms natively.
#[tauri::command]
pub async fn pair_invite(app: AppHandle, syncer: State<'_, Syncer>) -> Result<String> {
    if syncer.invite().is_none() {
        return Err(CommandError::code("NO_INVITE"));
    }
    if !confirm(&app, &SHOW_INVITE) {
        return Err(CommandError::code("CANCELLED"));
    }
    syncer
        .invite()
        .ok_or_else(|| CommandError::code("NO_INVITE"))
}

#[tauri::command]
pub async fn search_windows(looking: Looking, syncer: State<'_, Syncer>) -> Result<Vec<FoundView>> {
    Ok(syncer.search(looking).await?)
}

#[tauri::command]
pub async fn join_sync(index: usize, code: String, syncer: State<'_, Syncer>) -> Result<()> {
    let code = zeroize::Zeroizing::new(code);
    Ok(syncer.join_sync(index, &code)?)
}

#[tauri::command]
pub async fn join_pair(
    with: PairWith,
    name: Option<String>,
    replace: bool,
    syncer: State<'_, Syncer>,
) -> Result<()> {
    Ok(syncer.join_pair(&with, name.as_deref(), replace)?)
}

#[tauri::command]
pub fn answer_ask(id: u64, yes: bool, syncer: State<'_, Syncer>) -> Result<()> {
    Ok(syncer.answer(id, yes)?)
}

#[tauri::command]
pub fn close_session(syncer: State<'_, Syncer>) {
    syncer.close();
}

#[tauri::command]
pub fn rename_device(name: String, vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    vault.get().rename_device(name.trim())?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Removes a member for good, after the person confirms natively.
#[tauri::command]
pub async fn remove_device(
    fingerprint: String,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<()> {
    if !confirm(&app, &REMOVE_DEVICE) {
        return Err(CommandError::code("CANCELLED"));
    }
    vault.get().remove_device(&fingerprint)?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Leaves the sync group, after the person confirms natively.
#[tauri::command]
pub async fn leave_group(vault: State<'_, AppCore>, app: AppHandle) -> Result<()> {
    if !confirm(&app, &LEAVE_GROUP) {
        return Err(CommandError::code("CANCELLED"));
    }
    vault.get().leave_group()?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

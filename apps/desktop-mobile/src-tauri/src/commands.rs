//! The commands the quick panel calls (apps/desktop-mobile/src/panel/api.ts).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::clipboard::{CLEAR_AFTER, copy_then_clear};
use crate::core::{Core, CoreError, PanelEntry, PanelState};
use crate::desktop;
use crate::hotkeys::Hotkeys;
use crate::shortcuts::ShortcutMap;

/// The vault, shared by the commands.
pub struct AppCore(pub Mutex<Core>);

impl AppCore {
    pub(crate) fn get(&self) -> MutexGuard<'_, Core> {
        // A panic while holding the lock leaves the core usable: every change is saved before it returns.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// What the panel learns after a copy.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    clears_in_s: u64,
}

/// An error code the panel can show (`Clipboard`, `NeedsPassword`, …).
type CommandResult<T> = Result<T, String>;

fn code_of(error: CoreError) -> String {
    format!("{error:?}")
}

#[tauri::command]
pub fn panel_state(
    query: &str,
    vault: State<'_, AppCore>,
    hotkeys: State<'_, Hotkeys>,
) -> PanelState {
    let core = vault.get();
    let state = core.state(query);
    let places = list_places(&core);
    with_shortcuts(state, &hotkeys.map(), &places)
}

/// Each entry's place in the whole list, which decides its default shortcut.
pub(crate) fn list_places(core: &Core) -> HashMap<String, usize> {
    core.list("")
        .map(|all| {
            all.into_iter()
                .enumerate()
                .map(|(i, e)| (e.id, i))
                .collect()
        })
        .unwrap_or_default()
}

fn with_shortcuts(
    state: PanelState,
    map: &ShortcutMap,
    places: &HashMap<String, usize>,
) -> PanelState {
    let entries = state
        .entries
        .into_iter()
        .map(|entry| {
            let shortcut = places
                .get(&entry.id)
                .and_then(|&place| map.shown_for(&entry.id, place));
            PanelEntry { shortcut, ..entry }
        })
        .collect();
    PanelState { entries, ..state }
}

#[tauri::command]
pub fn copy_code(id: &str, vault: State<'_, AppCore>) -> CommandResult<CopyResult> {
    copy_entry(&vault, id)
}

/// Copies an entry's current code; it is cleared from the clipboard later.
pub(crate) fn copy_entry(vault: &AppCore, id: &str) -> CommandResult<CopyResult> {
    let code = vault.get().code(id).map_err(code_of)?;
    copy_then_clear(code, CLEAR_AFTER).map_err(|_| "Clipboard".to_owned())?;
    Ok(CopyResult {
        clears_in_s: CLEAR_AFTER.as_secs(),
    })
}

#[tauri::command]
pub fn next_hotp(id: &str, vault: State<'_, AppCore>) -> CommandResult<PanelEntry> {
    vault.get().next_hotp(id).map_err(code_of)
}

#[tauri::command]
pub fn unlock(password: String, vault: State<'_, AppCore>) -> PanelState {
    let password = zeroize::Zeroizing::new(password);
    vault.get().unlock(&password)
}

#[tauri::command]
pub fn open_main(page: Option<String>, app: AppHandle) {
    desktop::show_main(&app, page.as_deref());
}

#[tauri::command]
pub fn open_gate(app: AppHandle) {
    crate::gate::open_in_browser(&app);
}

#[tauri::command]
pub fn hide_panel(window: WebviewWindow) {
    let _ = window.hide();
}

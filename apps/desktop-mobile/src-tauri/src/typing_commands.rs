//! Typing a code into another app's field from the search panel (PRD FR-5.5, design/07 §2.3).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use zeroize::Zeroizing;

use crate::commands::{AppCore, CommandError, copy_entry};
use crate::hotkeys::Hotkeys;
use crate::typing::{self, Typing};

/// What the window learns after asking to type a code, or later when nothing could be typed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeResult {
    /// The code is being typed; the window has already been hidden.
    typed: bool,
    /// Why it was copied instead: `NO_PERMISSION`, `UNSUPPORTED`, `MAIN_OPEN` (macOS: giving focus back
    /// would hide the open main window) or `FAILED` (no key could be sent).
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback: Option<&'static str>,
    /// When copied: seconds until the clipboard is cleared, 0 when it is not.
    clears_in_s: u64,
}

/// Types the entry's current code into the app that had focus before OTPKeep, or copies it when typing is
/// not possible here.
#[tauri::command]
pub fn type_code(
    id: &str,
    window: WebviewWindow,
    app: AppHandle,
    vault: State<'_, AppCore>,
    hotkeys: State<'_, Hotkeys>,
) -> Result<TypeResult, CommandError> {
    let clear_after = hotkeys.settings().clear_after();
    if let Some(reason) = typing::status().fallback_code().or_else(|| main_open(&app)) {
        let copied = copy_entry(&vault, id, clear_after)?;
        return Ok(TypeResult {
            typed: false,
            fallback: Some(reason),
            clears_in_s: copied.clears_in_s,
        });
    }
    let code = vault.get().code(id).map_err(CommandError::from)?;
    vault.used(id);
    crate::tray::refresh(&app);
    crate::panels::hide(&app, &window);
    // Hiding the app gives focus back to the app the person was typing in.
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    typing::type_later(code, move |code| copy_instead(&app, code, clear_after));
    Ok(TypeResult {
        typed: true,
        fallback: None,
        clears_in_s: 0,
    })
}

/// macOS gives focus back to the previous app only by hiding all of OTPKeep, which would hide an open main
/// window too; then the code is copied instead.
fn main_open(app: &AppHandle) -> Option<&'static str> {
    let open = app
        .get_webview_window(crate::desktop::MAIN)
        .is_some_and(|main| main.is_visible().unwrap_or(false));
    (cfg!(target_os = "macos") && open).then_some("MAIN_OPEN")
}

/// Not a key could be typed: the code goes to the clipboard and the search panel comes back to say so.
fn copy_instead(app: &AppHandle, code: Zeroizing<String>, clear_after: Option<Duration>) {
    let Some(vault) = app.try_state::<AppCore>() else {
        return;
    };
    if vault
        .clipboard()
        .copy_then_clear(code, clear_after)
        .is_err()
    {
        tracing::warn!("the code could be neither typed nor copied");
        return;
    }
    let result = TypeResult {
        typed: false,
        fallback: Some("FAILED"),
        clears_in_s: clear_after.map_or(0, |d| d.as_secs()),
    };
    crate::panels::show_search_again(app);
    let _ = app.emit_to(crate::panels::SEARCH, "typing-failed", result);
}

/// Whether codes can be typed here, for the settings page.
#[tauri::command]
pub fn typing_status() -> Typing {
    typing::status()
}

/// Asks the system for the permission typing needs (macOS Accessibility).
#[tauri::command]
pub fn ask_typing_permission() -> Typing {
    typing::ask_permission()
}

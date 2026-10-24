//! Typing a code into another app's field from the search panel (PRD FR-5.5, design/07 §2.3).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::commands::{AppCore, CommandError, copy_entry};
use crate::hotkeys::Hotkeys;
use crate::typing::{self, Typing};

/// What the window learns after asking to type a code.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeResult {
    /// The code is being typed; the window has already been hidden.
    typed: bool,
    /// Why it was copied instead: `NO_PERMISSION` or `UNSUPPORTED`.
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
    if let Some(reason) = typing::status().fallback_code() {
        let copied = copy_entry(&vault, id, hotkeys.settings().clear_after())?;
        return Ok(TypeResult {
            typed: false,
            fallback: Some(reason),
            clears_in_s: copied.clears_in_s,
        });
    }
    let code = vault.get().code(id).map_err(CommandError::from)?;
    let _ = window.hide();
    // Hiding the app gives focus back to the app the person was typing in.
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    let _ = &app;
    typing::type_later(code);
    Ok(TypeResult {
        typed: true,
        fallback: None,
        clears_in_s: 0,
    })
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

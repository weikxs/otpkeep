//! The commands the main window uses to add accounts (design/07-platform.md §1; PRD FR-2, FR-6.6).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use keep_transfer::import::Choice;
use percent_encoding::percent_decode_str;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

use crate::commands::{AppCore, CommandError, blocking};
use crate::core::{ImportView, ManualForm, read_bytes};

/// Files larger than this are no import source OTPKeep reads.
const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;
/// The backup password travels percent-encoded in this header beside the file's raw bytes.
const PASSWORD_HEADER: &str = "x-backup-password";

type Result<T> = std::result::Result<T, CommandError>;

/// Pasted or typed text: a link, several links, or transfer codes.
#[tauri::command(async)]
pub fn stage_text(text: String, vault: State<'_, AppCore>) -> Result<ImportView> {
    let text = Zeroizing::new(text);
    Ok(vault.get().stage_text(&text)?)
}

/// A file or picture, sent as raw bytes; a backup's password in the `x-backup-password` header.
#[tauri::command]
pub async fn stage_file(request: Request<'_>, vault: State<'_, AppCore>) -> Result<ImportView> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(CommandError::code("BAD_REQUEST"));
    };
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(CommandError::code("FILE_TOO_LARGE"));
    }
    let password = request
        .headers()
        .get(PASSWORD_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(|encoded| {
            Zeroizing::new(percent_decode_str(encoded).decode_utf8_lossy().into_owned())
        });
    // Decoding a picture or deriving a backup's key takes a moment: it happens on a blocking thread, without
    // holding the core. The copy of the bytes (a picture may show secrets) is wiped when done.
    let bytes = Zeroizing::new(bytes.clone());
    let source =
        blocking(move || read_bytes(&bytes, password.as_deref().map(String::as_str))).await??;
    Ok(vault.get().stage_source(source)?)
}

/// One account typed into the form.
#[tauri::command(async)]
pub fn stage_manual(form: ManualForm, vault: State<'_, AppCore>) -> Result<ImportView> {
    Ok(vault.get().stage_manual(&form)?)
}

/// A staged import as it is now.
#[tauri::command]
pub fn staged_view(token: &str, vault: State<'_, AppCore>) -> Result<ImportView> {
    Ok(vault.get().staged_view(token)?)
}

/// Adds the chosen rows; returns how many were added.
#[tauri::command(async)]
pub fn commit_import(
    token: &str,
    fresh: Vec<bool>,
    duplicates: Vec<bool>,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> Result<usize> {
    let added = vault
        .get()
        .commit_staged(token, &Choice { fresh, duplicates })?;
    let _ = app.emit("vault-changed", ());
    Ok(added)
}

/// Drops a staged import.
#[tauri::command]
pub fn cancel_import(token: &str, vault: State<'_, AppCore>) {
    vault.get().cancel_staged(token);
}

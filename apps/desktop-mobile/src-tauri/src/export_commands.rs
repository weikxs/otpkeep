//! Exports from the main window (PRD FR-6.1–6.3, FR-6.7). The native layer asks for confirmation before
//! anything leaves unencrypted and asks where to save; the page never sees a file path or a file's bytes.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::commands::{AppCore, CommandError, blocking};
use crate::core::{ExportFormat, GoogleExport, LeftOut, QrImage};
use crate::native::{PLAIN_FILE, SHOW_CODES, confirm};
use crate::private_file::write_private;

type Result<T> = std::result::Result<T, CommandError>;

/// What a saved export holds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDone {
    pub count: usize,
    pub left_out: Vec<LeftOut>,
    /// The name it was saved under (not the folder).
    pub file_name: String,
}

/// Saves the chosen entries (all when `ids` is absent) as a file the person places.
#[tauri::command]
pub async fn export_file(
    format: ExportFormat,
    ids: Option<Vec<String>>,
    password: Option<String>,
    app: AppHandle,
    vault: State<'_, AppCore>,
) -> Result<ExportDone> {
    let password = password.map(Zeroizing::new);
    if format.is_plaintext() && !confirm(&app, &PLAIN_FILE) {
        return Err(CommandError::code("CANCELLED"));
    }
    // The entries are copied under the lock; a backup's key is then derived without holding the core.
    let job = vault.get().export_job(format, ids.as_deref())?;
    let file = blocking(move || job.finish(password.as_deref().map(String::as_str))).await??;
    let chosen = app
        .dialog()
        .file()
        .set_file_name(&file.file_name)
        .add_filter(file.extension, &[file.extension])
        .blocking_save_file()
        .ok_or_else(|| CommandError::code("CANCELLED"))?
        .into_path()
        .map_err(|_| CommandError::code("SAVE_FAILED"))?;
    write_private(&chosen, &file.bytes).map_err(|_| CommandError::code("SAVE_FAILED"))?;
    Ok(ExportDone {
        count: file.count,
        left_out: file.left_out,
        file_name: chosen.file_name().map_or_else(
            || file.file_name.clone(),
            |n| n.to_string_lossy().into_owned(),
        ),
    })
}

/// Google Authenticator transfer codes to show, after the person confirms natively.
#[tauri::command]
pub async fn export_google(
    ids: Option<Vec<String>>,
    app: AppHandle,
    vault: State<'_, AppCore>,
) -> Result<GoogleExport> {
    if !confirm(&app, &SHOW_CODES) {
        return Err(CommandError::code("CANCELLED"));
    }
    Ok(vault.get().export_google(ids.as_deref())?)
}

/// One entry's QR code to show, after the person confirms natively.
#[tauri::command]
pub async fn entry_qr(id: String, app: AppHandle, vault: State<'_, AppCore>) -> Result<QrImage> {
    if !confirm(&app, &SHOW_CODES) {
        return Err(CommandError::code("CANCELLED"));
    }
    Ok(vault.get().entry_qr(&id)?)
}

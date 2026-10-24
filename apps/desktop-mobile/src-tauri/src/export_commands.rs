//! Exports from the main window (PRD FR-6.1–6.3, FR-6.7). The native layer asks for confirmation before
//! anything leaves unencrypted and asks where to save; the page never sees a file path or a file's bytes.

use std::io::Write;
use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

use crate::commands::{AppCore, CommandError};
use crate::core::{ExportFormat, GoogleExport, LeftOut, QrImage};
use crate::native::{PLAIN_FILE, SHOW_CODES, confirm};

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
    let file = vault.get().export_file(
        format,
        ids.as_deref(),
        password.as_deref().map(String::as_str),
    )?;
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

/// Writes the file readable by its owner only, where the system allows it.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::write_private;

    #[test]
    fn writes_a_file_only_its_owner_can_read() {
        let dir = tempfile::Builder::new()
            .prefix("app-write-")
            .tempdir()
            .unwrap();
        let path = dir.path().join("links.txt");

        write_private(&path, b"otpauth://demo").unwrap();
        write_private(&path, b"again").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"again");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

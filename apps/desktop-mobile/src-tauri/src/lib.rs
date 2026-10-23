//! The OTPKeep app shell shared by desktop and mobile.

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Returns the [`tauri::Error`] reported when the app cannot start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default().run(tauri::generate_context!())
}

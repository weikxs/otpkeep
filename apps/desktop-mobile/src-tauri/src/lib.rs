//! The OTPKeep app shell shared by desktop and mobile.

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Returns the [`tauri::Error`] reported when the app cannot start.
pub fn try_run() -> Result<(), tauri::Error> {
    tauri::Builder::default().run(tauri::generate_context!())
}

/// Entry point on iOS and Android, where the platform calls the app and nothing receives a result.
///
/// A startup failure is reported instead of being dropped.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = try_run() {
        report_startup_failure(&error);
    }
}

// Until logging lands (M1-26), stderr is the only channel; it reaches the device console. Tracked as DEBT-002.
#[allow(
    clippy::print_stderr,
    reason = "the app failed to start, so stderr is the only place left to report it"
)]
fn report_startup_failure(error: &tauri::Error) {
    eprintln!("OTPKeep could not start: {error}");
}

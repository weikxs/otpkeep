//! Desktop entry point of the OTPKeep app.

// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

#[allow(
    clippy::print_stderr,
    reason = "the app failed to start, so stderr is the only place left to report it"
)]
fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    otpkeep_app_lib::large_frees::without_large_cache();
    match otpkeep_app_lib::try_run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("OTPKeep could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

//! The OTPKeep app shell shared by desktop and mobile.

#[cfg(desktop)]
mod clipboard;
#[cfg(desktop)]
mod commands;
#[cfg(desktop)]
mod core;
#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
mod gate;
#[cfg(desktop)]
mod hotkeys;
#[cfg(desktop)]
mod icon;
#[cfg(desktop)]
mod settings;
#[cfg(desktop)]
mod shortcuts;

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Returns the [`tauri::Error`] reported when the app cannot start.
pub fn try_run() -> Result<(), tauri::Error> {
    builder().run(tauri::generate_context!())
}

#[cfg(desktop)]
fn builder() -> tauri::Builder<tauri::Wry> {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use keep_platform::{Keystore, MemoryKeystore, SystemKeystore, data_dir};
    use keep_vault::service::system_now_ms;
    use keep_vault::store::VaultPaths;

    /// Set to `memory` to use an empty in-memory keystore instead of the system one (for tests).
    const KEYSTORE_OVERRIDE: &str = "OTPKEEP_KEYSTORE";
    /// How long to wait for another OTPKeep process to release the data.
    const LOCK_WAIT: Duration = Duration::from_secs(2);

    let keystore: Box<dyn Keystore> = if std::env::var(KEYSTORE_OVERRIDE).as_deref() == Ok("memory")
    {
        Box::new(MemoryKeystore::new())
    } else {
        Box::new(SystemKeystore::new())
    };
    let dir = data_dir();
    let hotkeys = hotkeys::Hotkeys::load(settings::SettingsFile::new(dir.as_deref()));
    let core = core::Core::start(
        dir.map(|dir| VaultPaths::new(&dir)),
        keystore,
        Arc::new(system_now_ms),
        LOCK_WAIT,
    );
    tauri::Builder::default()
        .plugin(hotkeys::plugin())
        .manage(commands::AppCore(Mutex::new(core)))
        .manage(hotkeys)
        .manage(gate::GateServer::default())
        .invoke_handler(tauri::generate_handler![
            commands::panel_state,
            commands::copy_code,
            commands::next_hotp,
            commands::unlock,
            commands::open_main,
            commands::open_gate,
            commands::hide_panel,
        ])
        .setup(|app| {
            desktop::setup(app.handle())?;
            hotkeys::register_all(app.handle());
            Ok(())
        })
}

#[cfg(mobile)]
fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
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

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
mod export_commands;
#[cfg(desktop)]
mod gate;
#[cfg(desktop)]
mod hotkeys;
#[cfg(desktop)]
mod icon;
#[cfg(desktop)]
mod import_commands;
#[cfg(desktop)]
mod key_commands;
#[cfg(desktop)]
mod log;
#[cfg(desktop)]
mod manage_commands;
#[cfg(desktop)]
mod native;
#[cfg(desktop)]
mod panels;
#[cfg(desktop)]
mod preferences;
#[cfg(desktop)]
mod private_file;
#[cfg(desktop)]
mod settings;
#[cfg(desktop)]
mod settings_commands;
#[cfg(desktop)]
mod shortcuts;
#[cfg(desktop)]
mod staging;
#[cfg(desktop)]
mod sync;
#[cfg(desktop)]
mod sync_commands;
#[cfg(desktop)]
mod tray;
#[cfg(desktop)]
mod tray_menu;
#[cfg(desktop)]
mod typing;
#[cfg(desktop)]
mod typing_commands;
#[cfg(desktop)]
mod unlock;

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Returns the [`tauri::Error`] reported when the app cannot start.
pub fn try_run() -> Result<(), tauri::Error> {
    let app = builder().build(tauri::generate_context!())?;
    app.run(on_event);
    Ok(())
}

/// The app lives in the menu bar: closing its last window (windows are closed when unused) does not end it;
/// only Quit, which asks with an exit code, does. A code waiting to be cleared from the clipboard is cleared when
/// the app quits, not left behind.
#[cfg(desktop)]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the signature Tauri's run loop calls"
)]
fn on_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    use tauri::Manager;
    match event {
        tauri::RunEvent::ExitRequested { code, api, .. } if !quits(code) => api.prevent_exit(),
        tauri::RunEvent::Exit => {
            if let Some(core) = app.try_state::<commands::AppCore>() {
                core.clipboard().clear_now();
            }
        }
        _ => {}
    }
}

/// Whether an exit request ends the app: Quit asks with a code, the last window closing asks without one.
#[cfg(desktop)]
const fn quits(code: Option<i32>) -> bool {
    code.is_some()
}

#[cfg(mobile)]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the signature Tauri's run loop calls"
)]
const fn on_event(_app: &tauri::AppHandle, _event: tauri::RunEvent) {}

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

    // First, so that what happens while the data opens is kept too.
    let log = log::Log::install();
    let keystore: Box<dyn Keystore> = if std::env::var(KEYSTORE_OVERRIDE).as_deref() == Ok("memory")
    {
        Box::new(MemoryKeystore::new())
    } else {
        Box::new(SystemKeystore::new())
    };
    let dir = data_dir();
    let hotkeys = hotkeys::Hotkeys::load(settings::SettingsFile::new(dir.as_deref()));
    let core = Arc::new(Mutex::new(core::Core::start(
        dir.map(|dir| VaultPaths::new(&dir)),
        keystore,
        Arc::new(system_now_ms),
        LOCK_WAIT,
    )));
    let session_core = Arc::clone(&core);
    tauri::Builder::default()
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppCore::new(core))
        .manage(hotkeys)
        .manage(gate::GateServer::default())
        .invoke_handler(tauri::generate_handler![
            commands::panel_state,
            commands::main_state,
            commands::edit_entry,
            commands::update_entry,
            commands::copy_code,
            commands::next_hotp,
            commands::unlock,
            commands::retry_open,
            commands::restore_previous,
            commands::open_main,
            commands::open_gate,
            import_commands::stage_text,
            import_commands::stage_file,
            import_commands::stage_manual,
            import_commands::staged_view,
            import_commands::commit_import,
            import_commands::cancel_import,
            manage_commands::set_pinned,
            manage_commands::set_tags,
            manage_commands::move_entry,
            manage_commands::trash_entry,
            manage_commands::restore_entry,
            manage_commands::purge_entry,
            manage_commands::trash_list,
            manage_commands::entry_history,
            manage_commands::batch_entries,
            key_commands::key_info,
            key_commands::preview_key,
            key_commands::replace_key,
            key_commands::rename_entry,
            key_commands::set_shortcut,
            settings_commands::get_settings,
            settings_commands::set_settings,
            settings_commands::set_search_panel_shortcut,
            typing_commands::type_code,
            typing_commands::typing_status,
            typing_commands::ask_typing_permission,
            export_commands::export_file,
            export_commands::export_google,
            export_commands::entry_qr,
            sync_commands::sync_state,
            sync_commands::open_sync_window,
            sync_commands::open_pair_window,
            sync_commands::pair_invite,
            sync_commands::search_windows,
            sync_commands::join_sync,
            sync_commands::join_pair,
            sync_commands::answer_ask,
            sync_commands::close_session,
            sync_commands::rename_device,
            sync_commands::remove_device,
            sync_commands::leave_group,
            commands::hide_panel,
        ])
        .setup(move |app| {
            use tauri::Manager;
            log.attach_to(app.handle());
            let notify = Arc::new(sync_commands::AppNotify(app.handle().clone()));
            app.manage(sync::Syncer::new(
                session_core,
                keep_link::Network::System,
                notify,
                Arc::new(system_now_ms),
            ));
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

#[cfg(all(test, desktop))]
mod tests {
    use super::quits;

    #[test]
    fn closing_the_last_window_leaves_the_app_in_the_menu_bar() {
        assert!(!quits(None));
    }

    #[test]
    fn quit_ends_the_app() {
        assert!(quits(Some(0)));
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

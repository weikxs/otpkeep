//! The commands the quick panel calls (apps/desktop-mobile/src/panel/api.ts).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use std::sync::MutexGuard;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use zeroize::Zeroizing;

use crate::clipboard::Clipboard;
use crate::core::{Core, CoreError, EntryUpdate, ImportProblem, PanelEntry, PanelState, Places};
use crate::desktop;
use crate::hotkeys::Hotkeys;
use crate::shortcuts::ShortcutMap;
use crate::sync::SharedCore;

// Commands declared `async` run on Tauri's thread pool, not on the thread that draws the windows, so a save
// (several `fsync`s) or deriving a key never freezes them. Reading commands stay on the main thread: they are
// quick, and answering them in order keeps a fast typist's results in order.

/// The vault, shared by the commands.
pub struct AppCore {
    core: SharedCore,
    /// One unlock at a time, so a second click cannot race the first for the vault's file lock.
    unlocking: tokio::sync::Mutex<()>,
    /// Where codes are copied, and cleared from later.
    clipboard: Clipboard,
}

impl AppCore {
    pub(crate) fn new(core: SharedCore) -> Self {
        Self {
            core,
            unlocking: tokio::sync::Mutex::new(()),
            clipboard: Clipboard::default(),
        }
    }

    pub(crate) const fn clipboard(&self) -> &Clipboard {
        &self.clipboard
    }

    pub(crate) fn get(&self) -> MutexGuard<'_, Core> {
        // A panic while holding the lock leaves the core usable: every change is saved before it returns.
        self.core
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Runs slow work (deriving a key, decoding a large picture) on a thread meant for blocking.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| CommandError::code("UNKNOWN"))
}

/// What the panel learns after a copy.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub(crate) clears_in_s: u64,
}

/// What a command reports when it fails: a code the window words, and sometimes a detail (a rule, a
/// position). Never a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl CommandError {
    pub(crate) const fn code(code: &'static str) -> Self {
        Self { code, detail: None }
    }

    pub(crate) fn with(code: &'static str, detail: String) -> Self {
        Self {
            code,
            detail: Some(detail),
        }
    }
}

impl From<CoreError> for CommandError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::NeedsPassword => Self::code("NEEDS_PASSWORD"),
            CoreError::NotFound => Self::code("NOT_FOUND"),
            CoreError::NoCodeYet => Self::code("NO_CODE_YET"),
            CoreError::Passcode => Self::code("PASSCODE"),
            CoreError::ReadOnly => Self::code("READ_ONLY"),
            CoreError::Save => Self::code("SAVE_FAILED"),
            CoreError::Invalid(rule) => Self::with("INVALID", rule.to_owned()),
            CoreError::SecretInvalid { at, found } => {
                Self::with("SECRET_INVALID", format!("{}:{found}", at + 1))
            }
            CoreError::Import(problem) => Self::code(import_code(problem)),
        }
    }
}

const fn import_code(problem: ImportProblem) -> &'static str {
    match problem {
        ImportProblem::NeedsBackupPassword => "BACKUP_PASSWORD_NEEDED",
        ImportProblem::WrongBackupPassword => "BACKUP_PASSWORD_WRONG",
        ImportProblem::BackupDamaged => "BACKUP_DAMAGED",
        ImportProblem::BackupNewer => "BACKUP_NEWER",
        ImportProblem::ImageTooLarge => "IMAGE_TOO_LARGE",
        ImportProblem::UnknownFormat => "UNKNOWN_FORMAT",
        ImportProblem::NotStaged => "IMPORT_GONE",
        ImportProblem::Changed => "IMPORT_CHANGED",
        ImportProblem::NoKeystore => "NO_KEYSTORE",
    }
}

type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub fn panel_state(
    query: &str,
    vault: State<'_, AppCore>,
    hotkeys: State<'_, Hotkeys>,
) -> PanelState {
    let (state, places) = vault.get().state_and_places(query, false);
    with_shortcuts(state, &hotkeys.map(), &places)
}

fn with_shortcuts(state: PanelState, map: &ShortcutMap, places: &Places) -> PanelState {
    let entries = state
        .entries
        .into_iter()
        .map(|entry| {
            let shortcut = places
                .get(&entry.id)
                .and_then(|&place| map.shown_for(&entry.id, place));
            let own_shortcut = map.own(&entry.id).map(str::to_owned);
            PanelEntry {
                shortcut,
                own_shortcut,
                ..entry
            }
        })
        .collect();
    PanelState { entries, ..state }
}

/// Every entry for the main window, with the same shortcuts and codes as the panel.
#[tauri::command]
pub fn main_state(
    query: &str,
    vault: State<'_, AppCore>,
    hotkeys: State<'_, Hotkeys>,
) -> PanelState {
    let (state, places) = vault.get().state_and_places(query, true);
    with_shortcuts(state, &hotkeys.map(), &places)
}

/// Changes an entry's name and / or note.
#[tauri::command(async)]
pub fn edit_entry(
    id: &str,
    title: Option<String>,
    note: Option<String>,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> CommandResult<()> {
    vault.get().edit(id, title, note)?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Saves everything the editor changed in one write.
#[tauri::command(async)]
pub fn update_entry(
    id: &str,
    update: EntryUpdate,
    vault: State<'_, AppCore>,
    app: AppHandle,
) -> CommandResult<()> {
    vault.get().update(id, update)?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub fn copy_code(
    id: &str,
    vault: State<'_, AppCore>,
    hotkeys: State<'_, Hotkeys>,
) -> CommandResult<CopyResult> {
    copy_entry(&vault, id, hotkeys.settings().clear_after())
}

/// Copies an entry's current code; it is cleared from the clipboard later.
pub(crate) fn copy_entry(
    vault: &AppCore,
    id: &str,
    clear_after: Option<Duration>,
) -> CommandResult<CopyResult> {
    let code = vault.get().code(id).map_err(CommandError::from)?;
    vault
        .clipboard
        .copy_then_clear(code, clear_after)
        .map_err(|_| CommandError::code("CLIPBOARD"))?;
    Ok(CopyResult {
        clears_in_s: clear_after.map_or(0, |d| d.as_secs()),
    })
}

#[tauri::command(async)]
pub fn next_hotp(id: &str, vault: State<'_, AppCore>) -> CommandResult<PanelEntry> {
    vault.get().next_hotp(id).map_err(CommandError::from)
}

#[tauri::command]
pub async fn unlock(password: String, vault: State<'_, AppCore>) -> CommandResult<PanelState> {
    unlock_with(&vault, Zeroizing::new(password)).await
}

/// Opens the vault with its master password. The key is derived (Argon2id, at full strength) on a blocking
/// thread without holding the core, so the panel and sync are not held up meanwhile.
pub(crate) async fn unlock_with(
    vault: &AppCore,
    password: Zeroizing<String>,
) -> CommandResult<PanelState> {
    let _one_at_a_time = vault.unlocking.lock().await;
    let target = vault.get().unlock_target();
    let opened = match target {
        Some(target) => Some(blocking(move || target.open(&password)).await?),
        None => None,
    };
    Ok(vault.get().finish_unlock(opened))
}

/// Tries again to open data that could not be opened (another OTPKeep, the keychain or the disk is back);
/// every window reloads.
#[tauri::command(async)]
pub fn retry_open(vault: State<'_, AppCore>, app: AppHandle) {
    vault.get().retry_open();
    let _ = app.emit("vault-changed", ());
}

/// Puts back the previous version of a damaged vault, after the person confirmed in the main window.
#[tauri::command(async)]
pub fn restore_previous(vault: State<'_, AppCore>, app: AppHandle) -> CommandResult<()> {
    vault.get().restore_previous()?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub fn open_main(page: Option<String>, app: AppHandle) {
    desktop::show_main(&app, page.as_deref());
}

#[tauri::command]
pub fn open_gate(app: AppHandle) {
    crate::gate::open_in_browser(&app);
}

#[tauri::command]
pub fn hide_panel(window: WebviewWindow) {
    let _ = window.hide();
}

#[cfg(test)]
mod tests {
    use super::CommandError;
    use crate::core::{CoreError, ImportProblem};

    fn json(error: CoreError) -> String {
        serde_json::to_string(&CommandError::from(error)).unwrap()
    }

    #[test]
    fn errors_reach_the_window_as_codes_with_a_detail_where_useful() {
        assert_eq!(
            json(CoreError::NeedsPassword),
            r#"{"code":"NEEDS_PASSWORD"}"#
        );
        assert_eq!(
            json(CoreError::Invalid("DIGITS")),
            r#"{"code":"INVALID","detail":"DIGITS"}"#
        );
        assert_eq!(
            json(CoreError::SecretInvalid { at: 4, found: '0' }),
            r#"{"code":"SECRET_INVALID","detail":"5:0"}"#
        );
        assert_eq!(
            json(CoreError::Import(ImportProblem::NeedsBackupPassword)),
            r#"{"code":"BACKUP_PASSWORD_NEEDED"}"#
        );
    }

    mod unlocking {
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        use keep_platform::MemoryKeystore;
        use keep_vault::open::create_with_master_password;
        use keep_vault::sealed::KdfParams;
        use keep_vault::store::VaultPaths;
        use zeroize::Zeroizing;

        use super::super::{AppCore, unlock_with};
        use crate::core::{Core, PanelStatus};

        const WAIT: Duration = Duration::from_millis(200);
        const PASSWORD: &str = "demo password";

        fn locked(dir: &std::path::Path) -> AppCore {
            // Weak parameters keep the test fast; the app always derives with the vault's own parameters.
            let params = KdfParams {
                salt: [6; 16],
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            };
            drop(
                create_with_master_password(&VaultPaths::new(dir), PASSWORD, params, WAIT).unwrap(),
            );
            let core = Core::start(
                Some(VaultPaths::new(dir)),
                Box::new(MemoryKeystore::new()),
                Arc::new(|| 1_700_000_020_000),
                WAIT,
            );
            AppCore::new(Arc::new(Mutex::new(core)))
        }

        fn unlock(vault: &AppCore, password: &str) -> PanelStatus {
            let password = Zeroizing::new(password.to_owned());
            tauri::async_runtime::block_on(unlock_with(vault, password))
                .unwrap()
                .status
        }

        #[test]
        fn unlocking_off_the_main_thread_gives_the_same_answers() {
            let dir = tempfile::Builder::new()
                .prefix("app-unlock-")
                .tempdir()
                .unwrap();
            let vault = locked(dir.path());

            assert_eq!(unlock(&vault, "wrong"), PanelStatus::NeedsPassword);
            assert_eq!(unlock(&vault, PASSWORD), PanelStatus::Ready);
            assert_eq!(unlock(&vault, "wrong"), PanelStatus::Ready);
            assert_eq!(vault.get().state("").status, PanelStatus::Ready);
        }

        #[test]
        fn the_core_answers_while_the_key_is_derived() {
            let dir = tempfile::Builder::new()
                .prefix("app-unlock-")
                .tempdir()
                .unwrap();
            let vault = locked(dir.path());
            let target = vault.get().unlock_target().unwrap();

            // The core is free between taking the target and finishing: other commands are answered.
            assert_eq!(vault.get().state("").status, PanelStatus::NeedsPassword);
            let opened = target.open(PASSWORD);
            let state = vault.get().finish_unlock(Some(opened));

            assert_eq!(state.status, PanelStatus::Ready);
            assert!(vault.get().unlock_target().is_none());
        }
    }
}

//! The commands the quick panel calls (apps/desktop-mobile/src/panel/api.ts).

// Tauri hands commands their state, handles and arguments by value.
#![allow(
    clippy::needless_pass_by_value,
    reason = "the signatures Tauri commands take"
)]

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use crate::clipboard::{CLEAR_AFTER, copy_then_clear};
use crate::core::{Core, CoreError, ImportProblem, PanelEntry, PanelState};
use crate::desktop;
use crate::hotkeys::Hotkeys;
use crate::shortcuts::ShortcutMap;

/// The vault, shared by the commands.
pub struct AppCore(pub Mutex<Core>);

impl AppCore {
    pub(crate) fn get(&self) -> MutexGuard<'_, Core> {
        // A panic while holding the lock leaves the core usable: every change is saved before it returns.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// What the panel learns after a copy.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    clears_in_s: u64,
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

    fn with(code: &'static str, detail: String) -> Self {
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
    let core = vault.get();
    let state = core.state(query);
    let places = list_places(&core);
    with_shortcuts(state, &hotkeys.map(), &places)
}

/// Each entry's place in the whole list, which decides its default shortcut.
pub(crate) fn list_places(core: &Core) -> HashMap<String, usize> {
    core.list("")
        .map(|all| {
            all.into_iter()
                .enumerate()
                .map(|(i, e)| (e.id, i))
                .collect()
        })
        .unwrap_or_default()
}

fn with_shortcuts(
    state: PanelState,
    map: &ShortcutMap,
    places: &HashMap<String, usize>,
) -> PanelState {
    let entries = state
        .entries
        .into_iter()
        .map(|entry| {
            let shortcut = places
                .get(&entry.id)
                .and_then(|&place| map.shown_for(&entry.id, place));
            PanelEntry { shortcut, ..entry }
        })
        .collect();
    PanelState { entries, ..state }
}

#[tauri::command]
pub fn copy_code(id: &str, vault: State<'_, AppCore>) -> CommandResult<CopyResult> {
    copy_entry(&vault, id)
}

/// Copies an entry's current code; it is cleared from the clipboard later.
pub(crate) fn copy_entry(vault: &AppCore, id: &str) -> CommandResult<CopyResult> {
    let code = vault.get().code(id).map_err(CommandError::from)?;
    copy_then_clear(code, CLEAR_AFTER).map_err(|_| CommandError::code("CLIPBOARD"))?;
    Ok(CopyResult {
        clears_in_s: CLEAR_AFTER.as_secs(),
    })
}

#[tauri::command]
pub fn next_hotp(id: &str, vault: State<'_, AppCore>) -> CommandResult<PanelEntry> {
    vault.get().next_hotp(id).map_err(CommandError::from)
}

#[tauri::command]
pub fn unlock(password: String, vault: State<'_, AppCore>) -> PanelState {
    let password = zeroize::Zeroizing::new(password);
    vault.get().unlock(&password)
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
}

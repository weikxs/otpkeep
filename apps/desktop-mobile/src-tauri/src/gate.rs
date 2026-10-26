//! The browser management page, backed by this app's vault and shortcuts (design/07-platform.md §2.5).

use std::sync::{Arc, Mutex, PoisonError};

use keep_gate::{Asset, Backend, Copied, EntryEdit, GateEntry, GateError, RunningGate};
use keep_vault::service::system_now_ms;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppCore;
use crate::core::{CoreError, EntryInfo, EntryKind, PanelEntry};
use crate::hotkeys::{
    Hotkeys, SetError, UpdateError, set_entry_shortcut, taken_detail, update_settings,
};
use crate::preferences::{SettingsPatch, view};

/// The page server, while it runs.
#[derive(Default)]
pub struct GateServer(Mutex<Option<RunningGate>>);

struct AppBackend {
    app: AppHandle,
}

impl AppBackend {
    fn core(&self) -> Result<tauri::State<'_, AppCore>, GateError> {
        self.app.try_state::<AppCore>().ok_or(GateError::Save)
    }

    fn shown(&self, infos: Vec<EntryInfo>) -> Result<Vec<GateEntry>, GateError> {
        let vault = self.core()?;
        let (state, places) = vault.get().state_and_places("", true);
        // The same codes as the panel and the main window.
        let mut rows: std::collections::HashMap<String, PanelEntry> = state
            .entries
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect();
        let hotkeys = self.app.try_state::<Hotkeys>().ok_or(GateError::Save)?;
        let (map, active) = (hotkeys.map(), hotkeys.active());
        Ok(infos
            .into_iter()
            .map(|info| {
                let shortcut = map.own(&info.id).map(str::to_owned);
                // A default the system refused is not offered as if it worked.
                let default_shortcut = places
                    .get(&info.id)
                    .and_then(|&p| map.default_for(p))
                    .filter(|s| active.contains(s));
                let row = rows.remove(&info.id);
                let (code, next_code, period_s, valid_until_ms) = row
                    .map_or((None, None, None, None), |r| {
                        (r.code, r.next_code, r.period_s, r.valid_until_ms)
                    });
                GateEntry {
                    id: info.id,
                    title: info.title,
                    issuer: info.issuer,
                    account: info.account,
                    note: info.note,
                    pinned: info.pinned,
                    kind: match info.kind {
                        EntryKind::Totp => "totp",
                        EntryKind::Hotp => "hotp",
                        EntryKind::Steam => "steam",
                    }
                    .to_owned(),
                    shortcut,
                    default_shortcut,
                    code,
                    next_code,
                    period_s,
                    valid_until_ms,
                }
            })
            .collect())
    }

    fn one(&self, id: &str) -> Result<GateEntry, GateError> {
        let info = self
            .core()?
            .get()
            .list("")
            .map_err(gate_error)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(GateError::NotFound)?;
        self.shown(vec![info])?.pop().ok_or(GateError::NotFound)
    }

    fn changed(&self) {
        let _ = self.app.emit("vault-changed", ());
    }
}

impl Backend for AppBackend {
    fn entries(&self, query: &str) -> Result<Vec<GateEntry>, GateError> {
        let infos = self.core()?.get().list(query).map_err(gate_error)?;
        self.shown(infos)
    }

    fn edit(&self, id: &str, edit: EntryEdit) -> Result<GateEntry, GateError> {
        self.core()?
            .get()
            .edit(id, edit.title, edit.note)
            .map_err(gate_error)?;
        self.changed();
        self.one(id)
    }

    fn set_shortcut(&self, id: &str, shortcut: Option<&str>) -> Result<GateEntry, GateError> {
        // The entry must exist (and the data be open) before a shortcut is given to it.
        self.one(id)?;
        set_entry_shortcut(&self.app, id, shortcut).map_err(|error| self.set_error(error))?;
        self.changed();
        self.one(id)
    }

    fn copy(&self, id: &str) -> Result<Copied, GateError> {
        let clear_after = self
            .app
            .try_state::<Hotkeys>()
            .ok_or(GateError::Save)?
            .settings()
            .clear_after();
        let vault = self.core()?;
        let code = vault.get().code(id).map_err(gate_error)?;
        // Same rules as the panel: kept out of clipboard history, cleared later.
        vault
            .clipboard()
            .copy_then_clear(code, clear_after)
            .map_err(|_| GateError::Clipboard)?;
        Ok(Copied {
            clears_in_s: clear_after.map_or(0, |d| d.as_secs()),
        })
    }

    fn language(&self) -> String {
        self.app
            .try_state::<Hotkeys>()
            .and_then(|h| h.settings().language)
            .unwrap_or_else(|| "system".to_owned())
    }

    fn set_language(&self, language: &str) -> Result<String, GateError> {
        let patch = SettingsPatch {
            language: Some(language.to_owned()),
            ..SettingsPatch::default()
        };
        let settings = update_settings(&self.app, &patch).map_err(|error| match error {
            UpdateError::Invalid(rule) => GateError::Invalid(rule),
            UpdateError::Save => GateError::Save,
        })?;
        // OTPKeep's windows follow at once.
        let _ = self.app.emit("settings-changed", view(&settings));
        Ok(settings.language.unwrap_or_else(|| "system".to_owned()))
    }

    fn asset(&self, path: &str) -> Option<Asset> {
        if path != "gate.html" && !path.starts_with("assets/") {
            return None;
        }
        self.app
            .asset_resolver()
            .get(path.to_owned())
            .map(|asset| Asset {
                bytes: asset.bytes,
                mime: asset.mime_type,
            })
    }
}

impl AppBackend {
    fn set_error(&self, error: SetError) -> GateError {
        match error {
            SetError::Invalid => GateError::ShortcutInvalid,
            SetError::Rejected => GateError::ShortcutRejected,
            SetError::Save => GateError::Save,
            SetError::Taken(holder) => GateError::ShortcutTaken(taken_detail(&self.app, &holder)),
        }
    }
}

fn gate_error(error: CoreError) -> GateError {
    match error {
        CoreError::NeedsPassword => GateError::NeedsPassword,
        CoreError::NotFound => GateError::NotFound,
        CoreError::ReadOnly => GateError::ReadOnly,
        CoreError::Invalid(code) => GateError::Invalid(code),
        CoreError::NoCodeYet => GateError::NoCodeYet,
        CoreError::Passcode
        | CoreError::Save
        | CoreError::SecretInvalid { .. }
        | CoreError::Import(_) => GateError::Save,
    }
}

/// Starts the page server if needed and opens a new one-time link in the default browser.
pub fn open_in_browser(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match new_link(&app).await {
            Ok(link) => open_url(&link),
            Err(error) => tracing::warn!(%error, "the browser page could not be opened"),
        }
    });
}

async fn new_link(app: &AppHandle) -> Result<String, String> {
    let server = app
        .try_state::<GateServer>()
        .ok_or("no page server state")?;
    if let Some(link) = existing_link(&server)? {
        return Ok(link);
    }
    let backend = Arc::new(AppBackend { app: app.clone() });
    let gate = RunningGate::start(backend, Arc::new(system_now_ms))
        .await
        .map_err(|e| e.to_string())?;
    let link = gate
        .new_link()
        .map_err(|e| e.to_string())?
        .ok_or("the page server stopped at once")?;
    *server.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(gate);
    Ok(link)
}

fn existing_link(server: &GateServer) -> Result<Option<String>, String> {
    let running = server.0.lock().unwrap_or_else(PoisonError::into_inner);
    match running.as_ref() {
        Some(gate) if !gate.is_stopped() => gate.new_link().map_err(|e| e.to_string()),
        _ => Ok(None),
    }
}

/// Stops the page server, ending its session.
pub fn stop(app: &AppHandle) {
    if let Some(server) = app.try_state::<GateServer>()
        && let Some(gate) = server
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    {
        gate.stop();
    }
}

/// Opens `url` with the system's default browser.
fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(url).status();
    if !result.is_ok_and(|status| status.success()) {
        tracing::warn!("the default browser could not be started");
    }
}

//! The browser management page, backed by this app's vault and shortcuts (design/07-platform.md §2.5).

use std::sync::{Arc, Mutex, PoisonError};

use keep_gate::{Asset, Backend, EntryEdit, GateEntry, GateError, RunningGate};
use keep_vault::service::system_now_ms;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{AppCore, list_places};
use crate::core::{CoreError, EntryInfo, EntryKind};
use crate::hotkeys::{Hotkeys, SetError, set_entry_shortcut};
use crate::shortcuts::Holder;

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
        let places = list_places(&self.core()?.get());
        let map = self
            .app
            .try_state::<Hotkeys>()
            .map(|h| h.map())
            .ok_or(GateError::Save)?;
        Ok(infos
            .into_iter()
            .map(|info| {
                let shortcut = map.own(&info.id).map(str::to_owned);
                let default_shortcut = places.get(&info.id).and_then(|&p| map.default_for(p));
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
                    }
                    .to_owned(),
                    shortcut,
                    default_shortcut,
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
    /// Names who holds a taken shortcut, as the page shows it.
    fn set_error(&self, error: SetError) -> GateError {
        match error {
            SetError::Invalid => GateError::ShortcutInvalid,
            SetError::Rejected => GateError::ShortcutRejected,
            SetError::Save => GateError::Save,
            SetError::Taken(Holder::SearchPanel) => {
                GateError::ShortcutTaken("SEARCH_PANEL".to_owned())
            }
            SetError::Taken(Holder::Row(row)) => {
                GateError::ShortcutTaken(format!("ROW:{}", row + 1))
            }
            SetError::Taken(Holder::Entry(entry)) => {
                let title = self.one(&entry).map_or_else(|_| entry.clone(), |e| e.title);
                GateError::ShortcutTaken(format!("ENTRY:{title}"))
            }
        }
    }
}

fn gate_error(error: CoreError) -> GateError {
    match error {
        CoreError::NeedsPassword => GateError::NeedsPassword,
        CoreError::NotFound => GateError::NotFound,
        CoreError::ReadOnly => GateError::ReadOnly,
        CoreError::Invalid(code) => GateError::Invalid(code),
        CoreError::NoCodeYet
        | CoreError::Passcode
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

//! Registering the global shortcuts with the system and acting on them (PRD FR-5.2–5.4).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::commands::{AppCore, copy_entry};
use crate::settings::{Settings, SettingsFile};
use crate::shortcuts::{Holder, ShortcutError, ShortcutMap, Target};

/// The shortcuts of this device and what each registered one copies.
pub struct Hotkeys {
    file: SettingsFile,
    state: Mutex<HotkeyState>,
}

struct HotkeyState {
    settings: Settings,
    map: ShortcutMap,
    /// Registered shortcut id → what it copies.
    registered: HashMap<u32, Target>,
}

/// Why setting an entry's shortcut failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetError {
    Invalid,
    Taken(Holder),
    /// The system refused it; another app probably holds it.
    Rejected,
    Save,
}

impl Hotkeys {
    /// Loads this device's shortcuts; nothing is registered yet.
    #[must_use]
    pub fn load(file: SettingsFile) -> Self {
        let settings = file.load();
        let map = ShortcutMap::new(cfg!(target_os = "macos"), valid_only(&settings));
        Self {
            file,
            state: Mutex::new(HotkeyState {
                settings,
                map,
                registered: HashMap::new(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, HotkeyState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The current shortcuts.
    #[must_use]
    pub fn map(&self) -> ShortcutMap {
        self.state().map.clone()
    }
}

/// Stored shortcuts that this version cannot parse are left out (and kept in the file).
fn valid_only(settings: &Settings) -> std::collections::BTreeMap<String, String> {
    settings
        .entry_shortcuts
        .iter()
        .filter_map(|(entry, s)| {
            crate::shortcuts::canonical(s)
                .ok()
                .map(|c| (entry.clone(), c))
        })
        .collect()
}

/// The plugin, with one handler for every shortcut.
#[must_use]
pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(
            |app: &AppHandle<R>, _shortcut: &Shortcut, event: ShortcutEvent| {
                if event.state == ShortcutState::Pressed {
                    pressed(app, event.id);
                }
            },
        )
        .build()
}

/// Registers every shortcut. One that the system refuses is skipped and reported in the log.
pub fn register_all<R: Runtime>(app: &AppHandle<R>) {
    let Some(hotkeys) = app.try_state::<Hotkeys>() else {
        return;
    };
    let mut state = hotkeys.state();
    for (text, target) in state.map.targets() {
        if let Some(id) = register(app, &text) {
            state.registered.insert(id, target);
        } else {
            tracing::warn!("a global shortcut could not be registered");
        }
    }
}

/// Registers one shortcut; `None` when the system refuses it.
fn register<R: Runtime>(app: &AppHandle<R>, text: &str) -> Option<u32> {
    let shortcut: Shortcut = text.parse().ok()?;
    app.global_shortcut().register(shortcut).ok()?;
    Some(shortcut.id())
}

fn unregister<R: Runtime>(app: &AppHandle<R>, text: &str) -> Option<u32> {
    let shortcut: Shortcut = text.parse().ok()?;
    let _ = app.global_shortcut().unregister(shortcut);
    Some(shortcut.id())
}

/// Sets or removes `entry`'s own shortcut: checked, registered with the system, then saved. On any failure
/// the old shortcut stays registered and saved.
///
/// # Errors
///
/// Returns [`SetError`].
pub fn set_entry_shortcut<R: Runtime>(
    app: &AppHandle<R>,
    entry: &str,
    input: Option<&str>,
) -> Result<(), SetError> {
    let hotkeys = app.try_state::<Hotkeys>().ok_or(SetError::Save)?;
    let wanted = input
        .map(crate::shortcuts::canonical)
        .transpose()
        .map_err(|_| SetError::Invalid)?;
    let mut state = hotkeys.state();
    let old = state.map.own(entry).map(str::to_owned);
    if old == wanted {
        return Ok(());
    }
    let next = state
        .map
        .with(entry, wanted.as_deref())
        .map_err(|error| match error {
            ShortcutError::Invalid => SetError::Invalid,
            ShortcutError::Taken(holder) => SetError::Taken(holder),
        })?;
    if let Some(old) = &old
        && let Some(id) = unregister(app, old)
    {
        state.registered.remove(&id);
    }
    let restore = |state: &mut HotkeyState| {
        if let Some(old) = &old
            && let Some(id) = register(app, old)
        {
            state.registered.insert(id, Target::Entry(entry.to_owned()));
        }
    };
    let new_id = match &wanted {
        Some(text) => {
            let Some(id) = register(app, text) else {
                restore(&mut state);
                return Err(SetError::Rejected);
            };
            Some(id)
        }
        None => None,
    };
    let mut settings = state.settings.clone();
    settings.entry_shortcuts = next.entries().clone();
    if hotkeys.file.save(&settings).is_err() {
        if let Some(text) = &wanted {
            unregister(app, text);
        }
        restore(&mut state);
        return Err(SetError::Save);
    }
    if let Some(id) = new_id {
        state.registered.insert(id, Target::Entry(entry.to_owned()));
    }
    state.settings = settings;
    state.map = next;
    Ok(())
}

fn pressed<R: Runtime>(app: &AppHandle<R>, id: u32) {
    let Some(hotkeys) = app.try_state::<Hotkeys>() else {
        return;
    };
    let Some(target) = hotkeys.state().registered.get(&id).cloned() else {
        return;
    };
    let Some(core) = app.try_state::<AppCore>() else {
        return;
    };
    let entry = match target {
        Target::Entry(entry) => Some(entry),
        Target::Row(index) => core
            .get()
            .list("")
            .ok()
            .and_then(|entries| entries.into_iter().nth(index))
            .map(|e| e.id),
    };
    // A shortcut for an entry that no longer exists, or before the master password, does nothing.
    if let Some(entry) = entry
        && copy_entry(&core, &entry).is_err()
    {
        tracing::info!("a global shortcut found no code to copy");
    }
}

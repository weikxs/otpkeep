//! Registering the global shortcuts with the system and acting on them (PRD FR-5.2–5.4).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::commands::{AppCore, copy_entry};
use crate::preferences::{SettingsPatch, apply};
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
    /// Registered shortcut id → its text and what it copies.
    registered: HashMap<u32, (String, Target)>,
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
        let search = settings
            .search_shortcut_on()
            .and_then(|s| crate::shortcuts::canonical(&s).ok());
        let map = ShortcutMap::new(settings.row_shortcuts_on(), valid_only(&settings))
            .with_search(search);
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

    /// This device's settings.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.state().settings.clone()
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
            state.registered.insert(id, (text, target));
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
            state
                .registered
                .insert(id, (old.clone(), Target::Entry(entry.to_owned())));
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
    if let (Some(id), Some(text)) = (new_id, &wanted) {
        state
            .registered
            .insert(id, (text.clone(), Target::Entry(entry.to_owned())));
    }
    state.settings = settings;
    state.map = next;
    // A default this entry used to shadow (or now shadows) comes back (or goes).
    reregister_rows(app, &mut state);
    Ok(())
}

/// Sets the search panel's shortcut, or turns it off with `None`: checked against the other shortcuts,
/// registered with the system, then saved. On failure the old one stays.
///
/// # Errors
///
/// Returns [`SetError`].
pub fn set_search_shortcut<R: Runtime>(
    app: &AppHandle<R>,
    input: Option<&str>,
) -> Result<(), SetError> {
    let hotkeys = app.try_state::<Hotkeys>().ok_or(SetError::Save)?;
    let wanted = input
        .map(crate::shortcuts::canonical)
        .transpose()
        .map_err(|_| SetError::Invalid)?;
    let mut state = hotkeys.state();
    let old = state.map.search().map(str::to_owned);
    if old == wanted {
        return Ok(());
    }
    if let Some(text) = &wanted {
        let without = state.map.with_search(None);
        if let Some(holder) = without.holder(text, "") {
            return Err(SetError::Taken(holder));
        }
    }
    if let Some(old) = &old
        && let Some(id) = unregister(app, old)
    {
        state.registered.remove(&id);
    }
    let new_id = match &wanted {
        Some(text) => {
            let Some(id) = register(app, text) else {
                if let Some(old) = &old
                    && let Some(id) = register(app, old)
                {
                    state.registered.insert(id, (old.clone(), Target::Search));
                }
                return Err(SetError::Rejected);
            };
            Some(id)
        }
        None => None,
    };
    let mut settings = state.settings.clone();
    // Empty is "off", which is different from "not chosen" (the system's default).
    settings.search_shortcut = Some(wanted.clone().unwrap_or_default());
    if hotkeys.file.save(&settings).is_err() {
        if let Some(text) = &wanted {
            unregister(app, text);
        }
        if let Some(old) = &old
            && let Some(id) = register(app, old)
        {
            state.registered.insert(id, (old.clone(), Target::Search));
        }
        return Err(SetError::Save);
    }
    if let (Some(id), Some(text)) = (new_id, &wanted) {
        state.registered.insert(id, (text.clone(), Target::Search));
    }
    state.settings = settings;
    state.map = state.map.with_search(wanted);
    // A row default the old search shortcut shadowed comes back.
    reregister_rows(app, &mut state);
    Ok(())
}

/// Why the settings could not be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// A value breaks this rule (`LANGUAGE`, `THEME`, `CLIPBOARD`).
    Invalid(&'static str),
    Save,
}

/// Applies a change from the settings page, saves it, and turns the row shortcuts on or off at once.
///
/// # Errors
///
/// Returns [`UpdateError`]; nothing changes on failure.
pub fn update_settings<R: Runtime>(
    app: &AppHandle<R>,
    patch: &SettingsPatch,
) -> Result<Settings, UpdateError> {
    let hotkeys = app.try_state::<Hotkeys>().ok_or(UpdateError::Save)?;
    let mut state = hotkeys.state();
    let next = apply(&state.settings, patch).map_err(UpdateError::Invalid)?;
    hotkeys.file.save(&next).map_err(|_| UpdateError::Save)?;
    let rows_changed = next.row_shortcuts_on() != state.settings.row_shortcuts_on();
    state.settings = next.clone();
    if rows_changed {
        state.map = state.map.with_defaults(next.row_shortcuts_on());
        reregister_rows(app, &mut state);
    }
    Ok(next)
}

/// Unregisters every row default and registers the ones the current map has.
fn reregister_rows<R: Runtime>(app: &AppHandle<R>, state: &mut HotkeyState) {
    let rows: Vec<(u32, String)> = state
        .registered
        .iter()
        .filter(|(_, (_, target))| matches!(target, Target::Row(_)))
        .map(|(id, (text, _))| (*id, text.clone()))
        .collect();
    for (id, text) in rows {
        unregister(app, &text);
        state.registered.remove(&id);
    }
    for (text, target) in state.map.targets() {
        if matches!(target, Target::Row(_))
            && let Some(id) = register(app, &text)
        {
            state.registered.insert(id, (text, target));
        }
    }
}

/// Who holds a taken shortcut, as the windows word it: `ENTRY:<name>`, `ROW:<n>` or `SEARCH_PANEL`.
#[must_use]
pub fn taken_detail<R: Runtime>(app: &AppHandle<R>, holder: &Holder) -> String {
    match holder {
        Holder::SearchPanel => "SEARCH_PANEL".to_owned(),
        Holder::Row(row) => format!("ROW:{}", row + 1),
        Holder::Entry(entry) => {
            let title = app
                .try_state::<AppCore>()
                .and_then(|core| core.get().list("").ok())
                .and_then(|all| all.into_iter().find(|e| &e.id == entry))
                .map_or_else(|| entry.clone(), |e| e.title);
            format!("ENTRY:{title}")
        }
    }
}

fn pressed<R: Runtime>(app: &AppHandle<R>, id: u32) {
    let Some(hotkeys) = app.try_state::<Hotkeys>() else {
        return;
    };
    let Some((_, target)) = hotkeys.state().registered.get(&id).cloned() else {
        return;
    };
    if target == Target::Search {
        crate::desktop::show_search(app);
        return;
    }
    let clear_after = hotkeys.settings().clear_after();
    let Some(core) = app.try_state::<AppCore>() else {
        return;
    };
    let entry = match target {
        Target::Entry(entry) => Some(entry),
        Target::Search => None,
        Target::Row(index) => core
            .get()
            .list("")
            .ok()
            .and_then(|entries| entries.into_iter().nth(index))
            .map(|e| e.id),
    };
    // A shortcut for an entry that no longer exists, or before the master password, does nothing.
    if let Some(entry) = entry
        && copy_entry(&core, &entry, clear_after).is_err()
    {
        tracing::info!("a global shortcut found no code to copy");
    }
}

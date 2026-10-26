//! The menu bar icon and its menu, drawn by the system (`NSMenu` on macOS, a Win32 menu on Windows,
//! `AppIndicator` on Linux), so no web page runs while the app rests in the menu bar. What the menu lists is
//! worked out in [`crate::tray_menu`].
//!
//! The codes in it are brought up to date when a shown code ends (one timer, set for that moment), when the
//! data changes (`vault-changed`: an edit, a sync merge, unlocking), when the settings change, and when the
//! pointer comes onto the icon (macOS, Windows), just before the menu can open. Nothing ticks otherwise.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::async_runtime::JoinHandle;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Wry};

use crate::commands::{AppCore, copy_entry};
use crate::core::{PanelState, PanelStatus};
use crate::hotkeys::Hotkeys;
use crate::tray_menu::{self, Action, Line};
use crate::{desktop, icon, panels, unlock};

/// The menu bar icon's id.
pub const TRAY: &str = "otpkeep";
/// The codes are read again this long after the first shown one ends, so the new one is current.
const AFTER_CHANGE: Duration = Duration::from_millis(50);
/// Events after which the menu is drawn again.
const REFRESH_ON: [&str; 2] = ["vault-changed", "settings-changed"];

/// The menu as drawn now: its lines, the items made for them (in the same order), and the timer that brings
/// the codes up to date.
#[derive(Default)]
pub struct TrayMenu(Mutex<Drawn>);

#[derive(Default)]
struct Drawn {
    lines: Vec<Line>,
    items: Vec<MenuItem<Wry>>,
    timer: Option<JoinHandle<()>>,
}

impl TrayMenu {
    fn drawn(&self) -> MutexGuard<'_, Drawn> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Sets up the menu bar icon and its menu.
///
/// # Errors
///
/// Returns the [`tauri::Error`] when the icon or its menu cannot be created.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayMenu::default());
    let lines = tray_menu::lines(
        &menu_state(app),
        search_shortcut(app).as_deref(),
        chinese(app),
    );
    let (menu, items) = build(app, &lines)?;
    *app.state::<TrayMenu>().drawn() = Drawn {
        lines,
        items,
        timer: None,
    };
    TrayIconBuilder::with_id(TRAY)
        .icon(Image::new_owned(
            icon::tray_pixels(),
            icon::SIZE,
            icon::SIZE,
        ))
        .icon_as_template(true)
        .tooltip("OTPKeep")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| chosen(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Enter { .. } = event {
                refresh(tray.app_handle());
            }
        })
        .build(app)?;
    for event in REFRESH_ON {
        let handle = app.clone();
        app.listen_any(event, move |_| refresh(&handle));
    }
    refresh(app);
    Ok(())
}

/// Draws the menu again soon, on the thread that owns it. Never at once: the caller may hold the vault.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let app = handle.clone();
        if handle.run_on_main_thread(move || draw(&app)).is_err() {
            tracing::warn!("the menu bar menu could not be brought up to date");
        }
    });
}

/// Brings the menu up to date with the data and sets the timer for the next change of codes.
fn draw(app: &AppHandle) {
    let state = menu_state(app);
    if state.status != PanelStatus::NeedsPassword {
        // Unlocked here or in another window: the password window has done its work.
        unlock::close(app);
    }
    let lines = tray_menu::lines(&state, search_shortcut(app).as_deref(), chinese(app));
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let mut drawn = menu.drawn();
    if tray_menu::same_shape(&drawn.lines, &lines) {
        reword(&drawn, &lines);
        drawn.lines = lines;
    } else {
        match replace(app, &lines) {
            Ok(items) => {
                drawn.lines = lines;
                drawn.items = items;
            }
            Err(error) => tracing::warn!(%error, "the menu bar menu could not be drawn"),
        }
    }
    if let Some(timer) = drawn.timer.take() {
        timer.abort();
    }
    drawn.timer = tray_menu::next_change_ms(&state).map(|at| {
        let wait = Duration::from_millis(at.saturating_sub(state.now_ms)) + AFTER_CHANGE;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(wait).await;
            refresh(&app);
        })
    });
}

/// Changes only the words of items whose words changed (new codes), keeping the menu as it is, even open.
fn reword(drawn: &Drawn, lines: &[Line]) {
    let items = lines.iter().filter_map(|line| match line {
        Line::Item(item) => Some(item),
        Line::Separator => None,
    });
    let before = drawn.lines.iter().filter_map(|line| match line {
        Line::Item(item) => Some(item),
        Line::Separator => None,
    });
    for ((new, old), item) in items.zip(before).zip(&drawn.items) {
        if new.text != old.text {
            let _ = item.set_text(&new.text);
        }
    }
}

/// Gives the icon a new menu for `lines`.
fn replace(app: &AppHandle, lines: &[Line]) -> tauri::Result<Vec<MenuItem<Wry>>> {
    let (menu, items) = build(app, lines)?;
    if let Some(tray) = app.tray_by_id(TRAY) {
        tray.set_menu(Some(menu))?;
    }
    Ok(items)
}

fn build(app: &AppHandle, lines: &[Line]) -> tauri::Result<(Menu<Wry>, Vec<MenuItem<Wry>>)> {
    let menu = Menu::new(app)?;
    let mut items = Vec::new();
    for line in lines {
        match line {
            Line::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Line::Item(item) => {
                let made = MenuItem::with_id(
                    app,
                    item.id.as_str(),
                    &item.text,
                    item.enabled,
                    item.accelerator.as_deref(),
                )?;
                menu.append(&made)?;
                items.push(made);
            }
        }
    }
    Ok((menu, items))
}

/// What the menu shows, with each entry's own shortcut where the system accepted it.
fn menu_state(app: &AppHandle) -> PanelState {
    let state = app.state::<AppCore>().menu_state();
    let Some(hotkeys) = app.try_state::<Hotkeys>() else {
        return state;
    };
    let (map, active) = (hotkeys.map(), hotkeys.active());
    let entries = state
        .entries
        .into_iter()
        .map(|entry| {
            let own = map.own(&entry.id).map(str::to_owned);
            let shortcut = own.clone().filter(|own| active.contains(own));
            crate::core::PanelEntry {
                shortcut,
                own_shortcut: own,
                ..entry
            }
        })
        .collect();
    PanelState { entries, ..state }
}

/// The search panel's shortcut, when the system accepted it.
fn search_shortcut(app: &AppHandle) -> Option<String> {
    let hotkeys = app.try_state::<Hotkeys>()?;
    let search = hotkeys.map().search().map(str::to_owned)?;
    hotkeys.active().contains(&search).then_some(search)
}

fn chinese(app: &AppHandle) -> bool {
    crate::native::chinese(app)
}

/// Does what the chosen item says.
fn chosen(app: &AppHandle, id: &str) {
    let Some(action) = Action::of(id) else {
        return;
    };
    match action {
        Action::Copy(entry) => copy(app, &entry),
        Action::NextCode(entry) => next_code(app, entry),
        Action::Search => panels::show_search(app),
        Action::Open | Action::Restore => desktop::show_main(app, None),
        Action::Manage => crate::gate::open_in_browser(app),
        Action::Sync => desktop::show_main(app, Some("sync")),
        Action::Settings => desktop::show_main(app, Some("settings")),
        Action::Unlock => unlock::show(app),
        Action::Retry => retry(app),
        Action::Quit => {
            crate::gate::stop(app);
            app.exit(0);
        }
    }
}

/// Copies the entry's current code; it is cleared from the clipboard later, as from any window.
fn copy(app: &AppHandle, entry: &str) {
    let (Some(core), Some(hotkeys)) = (app.try_state::<AppCore>(), app.try_state::<Hotkeys>())
    else {
        return;
    };
    if copy_entry(&core, entry, hotkeys.settings().clear_after()).is_err() {
        tracing::info!("the chosen menu entry had no code to copy");
    }
    // It is now the most recently used.
    refresh(app);
}

/// Makes a counter-based entry's next code and copies it. Saving the new count writes the vault, so it is done
/// off the thread that draws the menu.
fn next_code(app: &AppHandle, entry: String) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(core) = app.try_state::<AppCore>() else {
            return;
        };
        if core.get().next_hotp(&entry).is_err() {
            tracing::info!("the next counter code could not be made");
            return;
        }
        let _ = app.emit("vault-changed", ());
        copy(&app, &entry);
    });
}

/// Tries again to open the data; opening may wait for another OTPKeep, so it is done off the menu's thread.
fn retry(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(core) = app.try_state::<AppCore>() {
            core.get().retry_open();
        }
        let _ = app.emit("vault-changed", ());
    });
}

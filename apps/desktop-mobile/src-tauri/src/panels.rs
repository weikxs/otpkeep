//! The quick panel and the search panel: made when first asked for and closed again some time after they hide,
//! so an app resting in the menu bar keeps no web page in memory. The quick panel is made at startup and kept
//! for [`KEEP_PANEL`] after it hides, so it usually opens at once; the search panel is made when its shortcut is
//! pressed and kept only for [`KEEP_SEARCH`], long enough to come back when a code could not be typed.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, Runtime, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

/// The quick panel's window label (tauri.<os>.conf.json).
pub const PANEL: &str = "panel";
/// The search panel's window label (design/04 §6).
pub const SEARCH: &str = "search";
/// How long the quick panel is kept after it hides.
const KEEP_PANEL: Duration = Duration::from_mins(10);
/// How long the search panel is kept after it hides: typing a code reports a failure within a second or so.
const KEEP_SEARCH: Duration = Duration::from_secs(20);
/// The search panel sits this far down the screen, like other launchers.
const SEARCH_FROM_TOP: f64 = 0.22;
/// Gap between the menu bar and the panel, in physical pixels.
const PANEL_GAP: f64 = 6.0;
/// A click on the icon right after the panel hid on blur was the click that hid it.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// Counts each window's hidings, so a window shown again (or hidden again) before its time is up is not closed
/// by an earlier hiding.
#[derive(Default)]
pub struct Reaper(Mutex<HashMap<String, u64>>);

impl Reaper {
    /// The window hid; returns the ticket that may close it later.
    pub fn hidden(&self, label: &str) -> u64 {
        self.bump(label)
    }

    /// The window was shown: earlier tickets are void.
    pub fn shown(&self, label: &str) {
        self.bump(label);
    }

    /// Whether `ticket` is still the window's latest hiding.
    pub fn still_due(&self, label: &str, ticket: u64) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(label)
            == Some(&ticket)
    }

    fn bump(&self, label: &str) -> u64 {
        let mut counts = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let count = counts.entry(label.to_owned()).or_insert(0);
        *count += 1;
        *count
    }
}

/// When the panel last hid because it lost focus.
#[derive(Default)]
struct LastBlur(Mutex<Option<Instant>>);

/// Readies the panels' state and makes the quick panel, hidden, so its first opening is quick.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    app.manage(Reaper::default());
    app.manage(LastBlur::default());
    if let Some(panel) = window(app, PANEL) {
        hide(app, &panel);
    }
}

/// How long a hidden window is kept before it is closed.
fn keep_for(label: &str) -> Option<Duration> {
    match label {
        PANEL => Some(KEEP_PANEL),
        SEARCH => Some(KEEP_SEARCH),
        _ => None,
    }
}

/// The window `label`, made from its configuration when it does not exist (yet, or any more).
pub fn window<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(label) {
        return Some(window);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)?
        .clone();
    let built =
        WebviewWindowBuilder::from_config(app, &config).and_then(WebviewWindowBuilder::build);
    let window = match built {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(%error, label, "a panel could not be opened");
            return None;
        }
    };
    let (handle, hidden) = (app.clone(), window.clone());
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            if hidden.label() == PANEL
                && let Some(blur) = handle.try_state::<LastBlur>()
            {
                *blur.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
            }
            hide(&handle, &hidden);
        }
        // Closed through the window manager: hidden like any other way of leaving it, and closed later.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle, &hidden);
        }
        _ => {}
    });
    Some(window)
}

/// Hides a panel and closes it once it has stayed hidden for its time.
pub fn hide<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let _ = window.hide();
    let label = window.label().to_owned();
    let (Some(reaper), Some(keep)) = (app.try_state::<Reaper>(), keep_for(&label)) else {
        return;
    };
    let ticket = reaper.hidden(&label);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(keep).await;
        let due = app
            .try_state::<Reaper>()
            .is_some_and(|r| r.still_due(&label, ticket));
        if let Some(window) = app.get_webview_window(&label)
            && due
            && !window.is_visible().unwrap_or(false)
        {
            let _ = window.destroy();
        }
    });
}

/// Hides the panel `label` when it is open; nothing when it does not exist.
pub fn hide_label<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label)
        && window.is_visible().unwrap_or(false)
    {
        hide(app, &window);
    }
}

/// Shows a panel and keeps it from being closed while it is on screen.
fn reveal<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, event: &str) {
    if let Some(reaper) = app.try_state::<Reaper>() {
        reaper.shown(window.label());
    }
    let _ = window.show();
    let _ = window.set_focus();
    // A panel made just now has not loaded yet and loads fresh anyway; one that was hidden starts afresh.
    let _ = window.emit(event, ());
}

/// Opens or closes the quick panel from a click on the menu bar icon at `icon`.
pub fn toggle_panel<R: Runtime>(app: &AppHandle<R>, icon: Rect) {
    if let Some(panel) = app.get_webview_window(PANEL)
        && panel.is_visible().unwrap_or(false)
    {
        hide(app, &panel);
        return;
    }
    let just_hid = app.try_state::<LastBlur>().is_some_and(|blur| {
        blur.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
    });
    if !just_hid {
        reveal_panel(app, Some(icon));
    }
}

/// Shows the quick panel under the menu bar icon, or in the middle of the screen where the icon's place is
/// unknown (Linux), for example to ask for the master password when a shortcut is pressed before it was entered.
pub fn show_panel<R: Runtime>(app: &AppHandle<R>) {
    let icon = app
        .tray_by_id(crate::desktop::TRAY)
        .and_then(|tray| tray.rect().ok().flatten());
    reveal_panel(app, icon);
}

fn reveal_panel<R: Runtime>(app: &AppHandle<R>, icon: Option<Rect>) {
    let Some(panel) = window(app, PANEL) else {
        return;
    };
    match icon {
        Some(icon) => place_under(app, &panel, icon),
        None => place_centred(app, &panel, None),
    }
    reveal(app, &panel, "panel-shown");
}

/// Shows the search panel in the middle of the screen the pointer is on, or hides it when it is open.
pub fn show_search<R: Runtime>(app: &AppHandle<R>) {
    if let Some(search) = app.get_webview_window(SEARCH)
        && search.is_visible().unwrap_or(false)
    {
        hide(app, &search);
        return;
    }
    hide_label(app, PANEL);
    let Some(search) = window(app, SEARCH) else {
        return;
    };
    place_centred(app, &search, Some(SEARCH_FROM_TOP));
    reveal(app, &search, "search-shown");
}

/// Brings the search panel back where it was, without starting afresh, to tell the person something.
pub fn show_search_again<R: Runtime>(app: &AppHandle<R>) {
    if let Some(search) = app.get_webview_window(SEARCH) {
        if let Some(reaper) = app.try_state::<Reaper>() {
            reaper.shown(SEARCH);
        }
        let _ = search.show();
        let _ = search.set_focus();
    }
}

/// A window's outer size in physical pixels. A window never shown yet reports no size on Linux, which put the
/// panels' corner, not their middle, at the middle of the screen; then the configured size is used.
fn window_size<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) -> (f64, f64) {
    let reported = window
        .outer_size()
        .map_or((0.0, 0.0), |s| (f64::from(s.width), f64::from(s.height)));
    let scale = window.scale_factor().unwrap_or(1.0);
    let configured = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == window.label())
        .map_or((0.0, 0.0), |w| (w.width * scale, w.height * scale));
    known_size(reported, configured)
}

/// The reported size, or the configured one when the reported size is empty.
fn known_size(reported: (f64, f64), configured: (f64, f64)) -> (f64, f64) {
    if reported.0 > 0.0 && reported.1 > 0.0 {
        reported
    } else {
        configured
    }
}

/// Where a window of `size` goes on a screen at `origin` of `screen` size: centred across, and `from_top` of the
/// way down (its middle at the screen's middle when `None`).
fn centred(
    origin: (f64, f64),
    screen: (f64, f64),
    size: (f64, f64),
    from_top: Option<f64>,
) -> (f64, f64) {
    let x = origin.0 + (screen.0 - size.0) / 2.0;
    let y = from_top.map_or_else(
        || origin.1 + (screen.1 - size.1) / 2.0,
        |part| origin.1 + screen.1 * part,
    );
    (x, y.max(origin.1))
}

/// Centres a panel on the pointer's screen (`from_top` as in [`centred`]).
fn place_centred<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, from_top: Option<f64>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|at| window.monitor_from_point(at.x, at.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = window.center();
        return;
    };
    let origin = (
        f64::from(monitor.position().x),
        f64::from(monitor.position().y),
    );
    let screen = (
        f64::from(monitor.size().width),
        f64::from(monitor.size().height),
    );
    let (x, y) = centred(origin, screen, window_size(app, window), from_top);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Centres the panel under the icon, kept on the icon's screen.
fn place_under<R: Runtime>(app: &AppHandle<R>, panel: &WebviewWindow<R>, icon: Rect) {
    let scale = panel.scale_factor().unwrap_or(1.0);
    let at = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let panel_size = window_size(app, panel);
    let mut x = at.x + size.width / 2.0 - panel_size.0 / 2.0;
    let y = at.y + size.height + PANEL_GAP;
    if let Ok(Some(monitor)) = panel.monitor_from_point(at.x, at.y) {
        let left = f64::from(monitor.position().x);
        let right = left + f64::from(monitor.size().width);
        x = x.clamp(
            left + PANEL_GAP,
            (right - panel_size.0 - PANEL_GAP).max(left),
        );
    }
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

#[cfg(test)]
mod tests {
    use super::{Reaper, centred, keep_for, known_size};

    #[test]
    fn a_hidden_panel_is_closed_unless_shown_again() {
        let reaper = Reaper::default();

        let ticket = reaper.hidden("panel");
        assert!(reaper.still_due("panel", ticket));

        reaper.shown("panel");
        assert!(!reaper.still_due("panel", ticket));
    }

    #[test]
    fn only_the_latest_hiding_closes_a_panel() {
        let reaper = Reaper::default();

        let first = reaper.hidden("search");
        let second = reaper.hidden("search");

        assert!(!reaper.still_due("search", first));
        assert!(reaper.still_due("search", second));
        assert!(!reaper.still_due("panel", second));
    }

    #[test]
    fn only_the_panels_are_closed_when_hidden() {
        assert!(keep_for("panel").is_some_and(|d| d.as_secs() == 600));
        assert!(keep_for("search").is_some());
        assert!(keep_for("main").is_none());
    }

    #[test]
    fn a_window_never_shown_is_placed_by_its_configured_size() {
        assert!((known_size((0.0, 0.0), (680.0, 1120.0)).0 - 680.0).abs() < f64::EPSILON);
        assert!((known_size((340.0, 560.0), (680.0, 1120.0)).0 - 340.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_panel_is_centred_on_its_screen() {
        let (x, y) = centred((1920.0, 0.0), (1920.0, 1080.0), (340.0, 560.0), None);

        assert!((x - (1920.0 + 790.0)).abs() < f64::EPSILON);
        assert!((y - 260.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_search_panel_sits_above_the_middle() {
        let (x, y) = centred((0.0, 0.0), (1920.0, 1080.0), (620.0, 400.0), Some(0.22));

        assert!((x - 650.0).abs() < f64::EPSILON);
        assert!((y - 237.6).abs() < 1e-9);
    }
}

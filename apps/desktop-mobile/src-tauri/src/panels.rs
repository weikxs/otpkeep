//! The search panel, and closing hidden windows: the search panel is made when its shortcut is pressed and the
//! main window when it is asked for; each is closed for good [`KEEP_HIDDEN`] after it hides, so an app resting in
//! the menu bar keeps no web page in memory. The time is long enough for a window opened again soon to come back
//! at once, and for the search panel to come back when a code could not be typed.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

/// The search panel's window label (tauri.<os>.conf.json, design/04 §6).
pub const SEARCH: &str = "search";
/// How long a hidden window is kept before it is closed for good.
const KEEP_HIDDEN: Duration = Duration::from_secs(30);
/// The search panel sits this far down the screen, like other launchers.
const SEARCH_FROM_TOP: f64 = 0.22;

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

/// Readies the state that closes hidden windows. No window is made: each is made when first asked for.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    app.manage(Reaper::default());
}

/// How long a hidden window is kept before it is closed; `None` for windows closed at once.
fn keep_for(label: &str) -> Option<Duration> {
    match label {
        SEARCH | crate::desktop::MAIN => Some(KEEP_HIDDEN),
        _ => None,
    }
}

/// The window `label` was shown again: it is not closed by an earlier hiding.
pub fn shown<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(reaper) = app.try_state::<Reaper>() {
        reaper.shown(label);
    }
}

/// The panel `label`, made from its configuration when it does not exist (yet, or any more).
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
        WindowEvent::Focused(false) => hide(&handle, &hidden),
        // Closed through the window manager: hidden like any other way of leaving it, and closed later.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle, &hidden);
        }
        _ => {}
    });
    Some(window)
}

/// Hides a window and closes it once it has stayed hidden for its time.
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

/// Shows a panel and keeps it from being closed while it is on screen.
fn reveal<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, event: &str) {
    shown(app, window.label());
    let _ = window.show();
    let _ = window.set_focus();
    // A panel made just now has not loaded yet and loads fresh anyway; one that was hidden starts afresh.
    let _ = window.emit(event, ());
}

/// Shows the search panel in the middle of the screen the pointer is on, or hides it when it is open.
pub fn show_search<R: Runtime>(app: &AppHandle<R>) {
    if let Some(search) = app.get_webview_window(SEARCH)
        && search.is_visible().unwrap_or(false)
    {
        hide(app, &search);
        return;
    }
    let Some(search) = window(app, SEARCH) else {
        return;
    };
    place_centred(app, &search, Some(SEARCH_FROM_TOP));
    reveal(app, &search, "search-shown");
}

/// Brings the search panel back where it was, without starting afresh, to tell the person something.
pub fn show_search_again<R: Runtime>(app: &AppHandle<R>) {
    if let Some(search) = app.get_webview_window(SEARCH) {
        shown(app, SEARCH);
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

#[cfg(test)]
mod tests {
    use super::{Reaper, centred, keep_for, known_size};

    #[test]
    fn a_hidden_panel_is_closed_unless_shown_again() {
        let reaper = Reaper::default();

        let ticket = reaper.hidden("main");
        assert!(reaper.still_due("main", ticket));

        reaper.shown("main");
        assert!(!reaper.still_due("main", ticket));
    }

    #[test]
    fn only_the_latest_hiding_closes_a_panel() {
        let reaper = Reaper::default();

        let first = reaper.hidden("search");
        let second = reaper.hidden("search");

        assert!(!reaper.still_due("search", first));
        assert!(reaper.still_due("search", second));
        assert!(!reaper.still_due("main", second));
    }

    #[test]
    fn the_search_panel_and_the_main_window_are_closed_30_seconds_after_they_hide() {
        assert!(keep_for("search").is_some_and(|d| d.as_secs() == 30));
        assert!(keep_for("main").is_some_and(|d| d.as_secs() == 30));
        assert!(keep_for("unlock").is_none());
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

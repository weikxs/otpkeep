//! The menu bar icon, the quick panel under it and the main window (design/12 §1). The panel and the search
//! panel are made at startup, hidden, so they appear at once; the main window only while it is open.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::icon;

/// The quick panel's window label (tauri.conf.json).
pub const PANEL: &str = "panel";
/// The main window's label.
pub const MAIN: &str = "main";
/// The search panel's window label (design/04 §6).
pub const SEARCH: &str = "search";
/// The main window's page.
const MAIN_PAGE: &str = "index.html";
/// The main window's first size and smallest size, in logical pixels.
const MAIN_SIZE: (f64, f64) = (900.0, 640.0);
const MAIN_MIN_SIZE: (f64, f64) = (360.0, 480.0);
/// The search panel sits this far down the screen, like other launchers.
const SEARCH_FROM_TOP: f64 = 0.22;
/// Gap between the menu bar and the panel, in physical pixels.
const PANEL_GAP: f64 = 6.0;
/// A click on the icon right after the panel hid on blur was the click that hid it.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

/// When the panel last hid because it lost focus.
#[derive(Default)]
struct LastBlur(Mutex<Option<Instant>>);

/// Sets up the menu bar icon, its menu and the panel's behaviour.
///
/// # Errors
///
/// Returns the [`tauri::Error`] when the icon or its menu cannot be created.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)?;
    app.manage(LastBlur::default());

    let open = MenuItem::with_id(app, "open", "打开主窗口", true, None::<&str>)?;
    let manage = MenuItem::with_id(app, "manage", "在浏览器中管理…", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "同步…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 OTPKeep", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &manage, &sync, &settings, &separator, &quit])?;

    TrayIconBuilder::with_id("otpkeep")
        .icon(Image::new_owned(
            icon::tray_pixels(),
            icon::SIZE,
            icon::SIZE,
        ))
        .icon_as_template(true)
        .tooltip("OTPKeep")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app, None),
            "manage" => crate::gate::open_in_browser(app),
            "sync" => show_main(app, Some("sync")),
            "settings" => show_main(app, Some("settings")),
            "quit" => {
                crate::gate::stop(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), rect);
            }
        })
        .build(app)?;

    if let Some(panel) = app.get_webview_window(PANEL) {
        let handle = app.clone();
        let window = panel.clone();
        panel.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
                if let Some(blur) = handle.try_state::<LastBlur>() {
                    *blur
                        .0
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
                }
            }
        });
    }
    if let Some(search) = app.get_webview_window(SEARCH) {
        let window = search.clone();
        search.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        });
    }
    Ok(())
}

fn toggle_panel(app: &AppHandle, icon: Rect) {
    let Some(panel) = app.get_webview_window(PANEL) else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    let just_hid = app.try_state::<LastBlur>().is_some_and(|blur| {
        blur.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
    });
    if just_hid {
        return;
    }
    place_under(&panel, icon);
    let _ = panel.show();
    let _ = panel.set_focus();
    let _ = panel.emit("panel-shown", ());
}

/// Centres the panel under the icon, kept on the icon's screen.
fn place_under(panel: &WebviewWindow, icon: Rect) {
    let scale = panel.scale_factor().unwrap_or(1.0);
    let at = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let panel_size = panel
        .outer_size()
        .map_or((0.0, 0.0), |s| (f64::from(s.width), f64::from(s.height)));
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

/// Shows the main window, optionally at `page`. The window is made when it is asked for and closed for good
/// when the person closes it, so nothing of it runs while it is not open; the app stays in the menu bar.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(panel) = app.get_webview_window(PANEL) {
        let _ = panel.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let Some(main) = app.get_webview_window(MAIN) else {
        open_main(app, page);
        return;
    };
    let _ = main.show();
    let _ = main.unminimize();
    let _ = main.set_focus();
    let _ = main.emit("main-shown", ());
    if let Some(page) = page {
        let _ = main.emit("open-page", page);
    }
}

/// Makes the main window. A page to open goes in the address, since the page cannot hear an event before it
/// has loaded.
fn open_main(app: &AppHandle, page: Option<&str>) {
    let address = page.map_or_else(
        || MAIN_PAGE.to_owned(),
        |page| {
            format!(
                "{MAIN_PAGE}#page={}",
                utf8_percent_encode(page, NON_ALPHANUMERIC)
            )
        },
    );
    let built = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App(address.into()))
        .title("OTPKeep")
        .inner_size(MAIN_SIZE.0, MAIN_SIZE.1)
        .min_inner_size(MAIN_MIN_SIZE.0, MAIN_MIN_SIZE.1)
        .build();
    let main = match built {
        Ok(main) => main,
        Err(error) => {
            tracing::warn!(%error, "the main window could not be opened");
            return;
        }
    };
    let handle = app.clone();
    main.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            main_closed(&handle);
        }
    });
    let _ = main.set_focus();
}

/// The main window is gone: the app goes back to living in the menu bar only.
fn main_closed(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Shows the search panel in the middle of the screen the pointer is on, or hides it when it is open.
pub fn show_search<R: Runtime>(app: &AppHandle<R>) {
    let Some(search) = app.get_webview_window(SEARCH) else {
        return;
    };
    if search.is_visible().unwrap_or(false) {
        let _ = search.hide();
        return;
    }
    if let Some(panel) = app.get_webview_window(PANEL) {
        let _ = panel.hide();
    }
    place_centred(app, &search);
    let _ = search.show();
    let _ = search.set_focus();
    let _ = search.emit("search-shown", ());
}

/// Centres the search panel across the pointer's screen, a little above the middle.
fn place_centred<R: Runtime>(app: &AppHandle<R>, search: &WebviewWindow<R>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|at| search.monitor_from_point(at.x, at.y).ok().flatten())
        .or_else(|| search.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = search.center();
        return;
    };
    let size = search
        .outer_size()
        .map_or((0.0, 0.0), |s| (f64::from(s.width), f64::from(s.height)));
    let (left, top) = (
        f64::from(monitor.position().x),
        f64::from(monitor.position().y),
    );
    let (width, height) = (
        f64::from(monitor.size().width),
        f64::from(monitor.size().height),
    );
    let x = left + (width - size.0) / 2.0;
    let y = top + height * SEARCH_FROM_TOP;
    let _ = search.set_position(PhysicalPosition::new(x, y));
}

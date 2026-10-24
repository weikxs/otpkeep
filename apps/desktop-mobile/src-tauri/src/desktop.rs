//! The menu bar icon, the quick panel under it and the main window (design/12 §1).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Rect, WebviewWindow, WindowEvent};

use crate::icon;

/// The quick panel's window label (tauri.conf.json).
pub const PANEL: &str = "panel";
/// The main window's label.
pub const MAIN: &str = "main";
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
    if let Some(main) = app.get_webview_window(MAIN) {
        let handle = app.clone();
        let window = main.clone();
        main.on_window_event(move |event| {
            // Closing the main window keeps the app in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                #[cfg(not(target_os = "macos"))]
                let _ = &handle;
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

/// Shows the main window, optionally at `page`.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    if let Some(panel) = app.get_webview_window(PANEL) {
        let _ = panel.hide();
    }
    let Some(main) = app.get_webview_window(MAIN) else {
        return;
    };
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let _ = main.show();
    let _ = main.unminimize();
    let _ = main.set_focus();
    if let Some(page) = page {
        let _ = main.emit("open-page", page);
    }
}

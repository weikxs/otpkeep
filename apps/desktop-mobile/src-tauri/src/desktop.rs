//! The menu bar icon and the main window (design/12 §1); the panels are in [`crate::panels`]. The main window
//! exists only while it is open.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::commands::AppCore;
use crate::icon;
use crate::panels;

/// The main window's label.
pub const MAIN: &str = "main";
/// The menu bar icon's id.
pub const TRAY: &str = "otpkeep";
/// The main window's page.
const MAIN_PAGE: &str = "index.html";
/// The main window's first size and smallest size, in logical pixels.
const MAIN_SIZE: (f64, f64) = (900.0, 640.0);
const MAIN_MIN_SIZE: (f64, f64) = (360.0, 480.0);

/// The words of the menu bar icon's menu.
struct TrayLabels {
    /// Shows the quick panel: the only way to it on Linux, where clicks on the icon only open this menu.
    panel: &'static str,
    open: &'static str,
    manage: &'static str,
    sync: &'static str,
    settings: &'static str,
    quit: &'static str,
}

/// The menu's words in Chinese or English.
const fn tray_labels(chinese: bool) -> TrayLabels {
    if chinese {
        TrayLabels {
            panel: "显示快捷面板",
            open: "打开主窗口",
            manage: "在浏览器中管理…",
            sync: "同步…",
            settings: "设置…",
            quit: "退出 OTPKeep",
        }
    } else {
        TrayLabels {
            panel: "Show quick panel",
            open: "Open main window",
            manage: "Manage in the browser…",
            sync: "Sync…",
            settings: "Settings…",
            quit: "Quit OTPKeep",
        }
    }
}

/// The menu's items, kept to reword them when the language changes.
struct TrayMenu {
    panel: MenuItem<tauri::Wry>,
    open: MenuItem<tauri::Wry>,
    manage: MenuItem<tauri::Wry>,
    sync: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

/// Rewords the menu bar icon's menu in the language now chosen.
pub fn relabel_tray(app: &AppHandle) {
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let labels = tray_labels(crate::native::chinese(app));
    for (item, label) in [
        (&menu.panel, labels.panel),
        (&menu.open, labels.open),
        (&menu.manage, labels.manage),
        (&menu.sync, labels.sync),
        (&menu.settings, labels.settings),
        (&menu.quit, labels.quit),
    ] {
        let _ = item.set_text(label);
    }
}

/// Sets up the menu bar icon, its menu and the panel's behaviour.
///
/// # Errors
///
/// Returns the [`tauri::Error`] when the icon or its menu cannot be created.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)?;
    panels::setup(app);

    let labels = tray_labels(crate::native::chinese(app));
    let item = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    let items = TrayMenu {
        panel: item("panel", labels.panel)?,
        open: item("open", labels.open)?,
        manage: item("manage", labels.manage)?,
        sync: item("sync", labels.sync)?,
        settings: item("settings", labels.settings)?,
        quit: item("quit", labels.quit)?,
    };
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &items.panel,
            &items.open,
            &items.manage,
            &items.sync,
            &items.settings,
            &separator,
            &items.quit,
        ],
    )?;
    app.manage(items);

    TrayIconBuilder::with_id(TRAY)
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
            "panel" => panels::show_panel(app),
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
                panels::toggle_panel(tray.app_handle(), rect);
            }
        })
        .build(app)?;

    Ok(())
}

/// Shows the main window, optionally at `page`. The window is made when it is asked for and closed for good
/// when the person closes it, so nothing of it runs while it is not open; the app stays in the menu bar.
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    panels::hide_label(app, panels::PANEL);
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

/// The main window is gone: imports it was previewing are dropped with their secrets, and the app goes back
/// to living in the menu bar only.
fn main_closed(app: &AppHandle) {
    if let Some(vault) = app.try_state::<AppCore>() {
        vault.get().cancel_all_staged();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

#[cfg(test)]
mod tests {
    use super::tray_labels;

    #[test]
    fn the_menu_has_words_in_both_languages() {
        let (zh, en) = (tray_labels(true), tray_labels(false));
        assert_eq!(zh.open, "打开主窗口");
        assert_eq!(en.open, "Open main window");
        assert_eq!(en.panel, "Show quick panel");
        assert_eq!(zh.panel, "显示快捷面板");
        assert_eq!(en.quit, "Quit OTPKeep");
        for label in [en.panel, en.open, en.quit] {
            assert!(label.is_ascii(), "{label}");
        }
        assert_ne!(zh.settings, en.settings);
    }
}

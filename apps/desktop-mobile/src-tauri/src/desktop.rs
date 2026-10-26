//! The main window (design/12 §1); the menu bar icon is in [`crate::tray`], the search panel in
//! [`crate::panels`]. The main window is made when asked for and, like the search panel, closed for good some
//! time after the person closes it, so an app resting in the menu bar runs no web page.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::commands::AppCore;
use crate::panels;

/// The main window's label.
pub const MAIN: &str = "main";
/// The main window's page.
const MAIN_PAGE: &str = "index.html";
/// The main window's first size and smallest size, in logical pixels.
const MAIN_SIZE: (f64, f64) = (900.0, 640.0);
const MAIN_MIN_SIZE: (f64, f64) = (360.0, 480.0);

/// Sets up the app in the menu bar: no Dock icon on macOS until the main window opens.
///
/// # Errors
///
/// Returns the [`tauri::Error`] when the menu bar icon or its menu cannot be created.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)?;
    panels::setup(app);
    crate::tray::setup(app)
}

/// Shows the main window, optionally at `page`, made now when it is not open (any more).
pub fn show_main(app: &AppHandle, page: Option<&str>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    let Some(main) = app.get_webview_window(MAIN) else {
        open_main(app, page);
        return;
    };
    panels::shown(app, MAIN);
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
    let (handle, window) = (app.clone(), main.clone());
    main.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // Hidden and closed for good later, so opening it again soon is quick.
            api.prevent_close();
            main_hidden(&handle);
            panels::hide(&handle, &window);
        }
    });
    let _ = main.set_focus();
}

/// The main window was closed: imports it was previewing are dropped with their secrets at once, and the app
/// goes back to living in the menu bar only.
fn main_hidden(app: &AppHandle) {
    if let Some(vault) = app.try_state::<AppCore>() {
        vault.get().cancel_all_staged();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

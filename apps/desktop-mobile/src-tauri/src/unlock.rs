//! The small window that asks for the master password, opened from the menu bar menu or by a shortcut
//! pressed before the password was entered. It is closed for good once the data is open, so no web page is
//! left running for it.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The window's label.
pub const UNLOCK: &str = "unlock";
/// Its page.
const PAGE: &str = "unlock.html";
/// Its size in logical pixels.
const SIZE: (f64, f64) = (380.0, 240.0);

/// The window's title in the person's language.
const fn title(chinese: bool) -> &'static str {
    if chinese {
        "解锁 OTPKeep"
    } else {
        "Unlock OTPKeep"
    }
}

/// Shows the password window, made now when it is not open yet.
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(UNLOCK) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, UNLOCK, WebviewUrl::App(PAGE.into()))
        .title(title(crate::native::chinese(app)))
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .center()
        .focused(true)
        // No website data: WebKit's networking helper then ends with the last window (see crate::desktop).
        .incognito(true)
        .build();
    match built {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(error) => tracing::warn!(%error, "the password window could not be opened"),
    }
}

/// Closes the password window for good, when it is open.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(UNLOCK) {
        let _ = window.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::title;

    #[test]
    fn the_title_is_worded_in_both_languages() {
        assert_eq!(title(false), "Unlock OTPKeep");
        assert_eq!(title(true), "解锁 OTPKeep");
    }
}

//! Copying a code: write it to the clipboard, keep it out of clipboard history apps, and clear it later if it
//! is still there (PRD FR-1.3; design/12 §6).

use std::time::Duration;

use arboard::Clipboard;
use zeroize::Zeroizing;

/// Puts `code` on the clipboard and clears it after `after`, unless something else was copied meanwhile;
/// with `None` it stays (the person turned clearing off).
///
/// # Errors
///
/// Returns the clipboard's error when the code could not be written.
pub fn copy_then_clear(
    code: Zeroizing<String>,
    after: Option<Duration>,
) -> Result<(), arboard::Error> {
    let mut clipboard = Clipboard::new()?;
    write_hidden(&mut clipboard, &code)?;
    let Some(after) = after else {
        return Ok(());
    };
    std::thread::Builder::new()
        .name("clipboard-clear".to_owned())
        .spawn(move || {
            std::thread::sleep(after);
            clear_if_unchanged(&code);
        })
        .map(drop)
        .map_err(|_| arboard::Error::Unknown {
            description: "could not schedule clearing".to_owned(),
        })
}

fn clear_if_unchanged(code: &str) {
    let Ok(mut clipboard) = Clipboard::new() else {
        return;
    };
    let current = clipboard.get_text().map(Zeroizing::new);
    if current.is_ok_and(|text| text.as_str() == code) {
        let _ = clipboard.clear();
    }
}

#[cfg(target_os = "macos")]
fn write_hidden(clipboard: &mut Clipboard, code: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtApple;
    clipboard.set().exclude_from_history().text(code)
}

#[cfg(target_os = "windows")]
fn write_hidden(clipboard: &mut Clipboard, code: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtWindows;
    clipboard
        .set()
        .exclude_from_monitoring()
        .exclude_from_cloud()
        .exclude_from_history()
        .text(code)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_hidden(clipboard: &mut Clipboard, code: &str) -> Result<(), arboard::Error> {
    clipboard.set_text(code)
}

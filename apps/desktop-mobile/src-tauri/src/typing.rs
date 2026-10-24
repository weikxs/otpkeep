//! Typing a code into the field that has focus, as if from the keyboard (PRD FR-5.5, design/07 §2.3).
//!
//! Only simulated key presses go out; nothing is read from the keyboard. Where typing is not allowed (no
//! Accessibility permission on macOS) or not possible (Wayland), the caller copies instead.

use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, NewConError, Settings};
use serde::Serialize;

/// Time for the previous app to take focus back after OTPKeep hides.
const FOCUS_SETTLE: Duration = Duration::from_millis(180);
/// Gap between digits, so fields that move to the next box on each key keep up.
const KEY_GAP: Duration = Duration::from_millis(25);

/// Whether codes can be typed on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Typing {
    Ready,
    /// macOS: OTPKeep needs Accessibility permission.
    NeedsPermission,
    /// Wayland, or the system refused a connection.
    Unsupported,
}

impl Typing {
    /// The code the windows show when falling back to copying.
    #[must_use]
    pub fn fallback_code(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::NeedsPermission => Some("NO_PERMISSION"),
            Self::Unsupported => Some("UNSUPPORTED"),
        }
    }
}

/// Checks without asking the person anything.
#[must_use]
pub fn status() -> Typing {
    if is_wayland(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    ) {
        return Typing::Unsupported;
    }
    match connect(false) {
        Ok(_) => Typing::Ready,
        Err(NewConError::NoPermission) => Typing::NeedsPermission,
        Err(_) => Typing::Unsupported,
    }
}

/// macOS: shows the system's request for Accessibility permission. Elsewhere it only checks.
#[must_use]
pub fn ask_permission() -> Typing {
    if status() == Typing::NeedsPermission {
        let _ = connect(true);
    }
    status()
}

/// Types `code` on a thread of its own, after the previous app had time to take focus back. The code is
/// wiped from memory afterwards.
pub fn type_later(code: zeroize::Zeroizing<String>) {
    let spawned = std::thread::Builder::new()
        .name("type-code".to_owned())
        .spawn(move || {
            std::thread::sleep(FOCUS_SETTLE);
            if type_now(&code).is_err() {
                tracing::warn!("the code could not be typed");
            }
        });
    if spawned.is_err() {
        tracing::warn!("could not start typing");
    }
}

fn type_now(code: &str) -> Result<(), enigo::InputError> {
    let mut enigo = connect(false).map_err(|_| enigo::InputError::Simulate("no connection"))?;
    for digit in code.chars() {
        enigo.key(Key::Unicode(digit), Direction::Click)?;
        std::thread::sleep(KEY_GAP);
    }
    Ok(())
}

fn connect(prompt: bool) -> Result<Enigo, NewConError> {
    Enigo::new(&Settings {
        open_prompt_to_get_permissions: prompt,
        // A modifier the person still holds does not change the digits (macOS).
        independent_of_keyboard_state: true,
        ..Settings::default()
    })
}

/// A Wayland session, where other apps' fields cannot be typed into (v1).
fn is_wayland(session_type: Option<&str>, wayland_display: bool) -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && (session_type.is_some_and(|t| t.eq_ignore_ascii_case("wayland")) || wayland_display)
}

#[cfg(test)]
mod tests {
    use super::{Typing, is_wayland};

    #[test]
    fn falls_back_with_a_reason_the_windows_understand() {
        assert_eq!(Typing::Ready.fallback_code(), None);
        assert_eq!(
            Typing::NeedsPermission.fallback_code(),
            Some("NO_PERMISSION")
        );
        assert_eq!(Typing::Unsupported.fallback_code(), Some("UNSUPPORTED"));
    }

    #[test]
    fn only_linux_wayland_sessions_are_unsupported() {
        let linux = cfg!(all(unix, not(target_os = "macos")));

        assert_eq!(is_wayland(Some("wayland"), false), linux);
        assert_eq!(is_wayland(None, true), linux);
        assert!(!is_wayland(Some("x11"), false));
        assert!(!is_wayland(None, false));
    }

    #[test]
    fn the_status_reaches_the_windows_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Typing::NeedsPermission).unwrap(),
            "\"needsPermission\""
        );
    }
}

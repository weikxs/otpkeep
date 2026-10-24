//! The settings page's settings: what the window sees and how a change is checked (design/10-reference.md §2).

use serde::{Deserialize, Serialize};

use crate::settings::{DEFAULT_CLEAR_S, DEFAULT_PANEL_OPACITY, Settings};

/// Shortest and longest clipboard clearing time; 0 turns clearing off.
const CLEAR_RANGE: std::ops::RangeInclusive<u16> = 10..=120;
/// The quick panel's opacity in percent: below 70 the codes get hard to read.
const OPACITY_RANGE: std::ops::RangeInclusive<u8> = 70..=100;

/// The settings as the window shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// `system`, `zh` or `en`.
    pub language: String,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// 0 when copied codes are never cleared.
    pub clipboard_clear_s: u16,
    pub row_shortcuts: bool,
    /// What `row_shortcuts` is on this system when never changed.
    pub row_shortcuts_default: bool,
    /// The quick panel's opacity in percent.
    pub panel_opacity: u8,
    /// The search panel's shortcut; absent when it has none.
    pub search_shortcut: Option<String>,
    /// What the search panel's shortcut is on this system when never changed.
    pub search_shortcut_default: Option<&'static str>,
    pub version: &'static str,
}

/// A change from the settings page; absent fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub language: Option<String>,
    pub theme: Option<String>,
    pub clipboard_clear_s: Option<u16>,
    pub row_shortcuts: Option<bool>,
    pub panel_opacity: Option<u8>,
}

#[must_use]
pub fn view(settings: &Settings) -> SettingsView {
    SettingsView {
        language: settings
            .language
            .clone()
            .unwrap_or_else(|| "system".to_owned()),
        theme: settings
            .theme
            .clone()
            .unwrap_or_else(|| "system".to_owned()),
        clipboard_clear_s: settings.clipboard_clear_s.unwrap_or(DEFAULT_CLEAR_S),
        row_shortcuts: settings.row_shortcuts_on(),
        row_shortcuts_default: cfg!(target_os = "macos"),
        panel_opacity: settings.panel_opacity.unwrap_or(DEFAULT_PANEL_OPACITY),
        search_shortcut: settings.search_shortcut_on(),
        search_shortcut_default: cfg!(target_os = "macos")
            .then_some(crate::shortcuts::SEARCH_PANEL_DEFAULT),
        version: env!("CARGO_PKG_VERSION"),
    }
}

/// The settings with `patch` applied; "system" removes a stored language or theme.
///
/// # Errors
///
/// Returns the rule that a value breaks: `LANGUAGE`, `THEME`, `CLIPBOARD` or `OPACITY`.
pub fn apply(settings: &Settings, patch: &SettingsPatch) -> Result<Settings, &'static str> {
    let mut next = settings.clone();
    if let Some(language) = &patch.language {
        next.language = match language.as_str() {
            "system" => None,
            "zh" | "en" => Some(language.clone()),
            _ => return Err("LANGUAGE"),
        };
    }
    if let Some(theme) = &patch.theme {
        next.theme = match theme.as_str() {
            "system" => None,
            "light" | "dark" => Some(theme.clone()),
            _ => return Err("THEME"),
        };
    }
    if let Some(seconds) = patch.clipboard_clear_s {
        if seconds != 0 && !CLEAR_RANGE.contains(&seconds) {
            return Err("CLIPBOARD");
        }
        next.clipboard_clear_s = Some(seconds);
    }
    if let Some(on) = patch.row_shortcuts {
        next.row_shortcuts = Some(on);
    }
    if let Some(opacity) = patch.panel_opacity {
        if !OPACITY_RANGE.contains(&opacity) {
            return Err("OPACITY");
        }
        next.panel_opacity = Some(opacity);
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SettingsPatch, apply, view};
    use crate::settings::Settings;

    #[test]
    fn shows_the_defaults() {
        let shown = view(&Settings::default());

        assert_eq!(
            (shown.language.as_str(), shown.theme.as_str()),
            ("system", "system")
        );
        assert_eq!(shown.clipboard_clear_s, 30);
        assert_eq!(shown.row_shortcuts, cfg!(target_os = "macos"));
        assert_eq!(
            Settings::default().clear_after(),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn applies_a_change_and_back_to_the_system() {
        let patch = SettingsPatch {
            language: Some("en".to_owned()),
            theme: Some("dark".to_owned()),
            clipboard_clear_s: Some(0),
            row_shortcuts: Some(false),
            panel_opacity: None,
        };

        let changed = apply(&Settings::default(), &patch).unwrap();

        assert_eq!(changed.language.as_deref(), Some("en"));
        assert_eq!(changed.clear_after(), None, "0 never clears");
        assert!(!changed.row_shortcuts_on());
        let back = SettingsPatch {
            language: Some("system".to_owned()),
            ..SettingsPatch::default()
        };
        assert_eq!(apply(&changed, &back).unwrap().language, None);
    }

    #[test]
    fn refuses_values_outside_the_rules() {
        let base = Settings::default();
        let patch = |p: SettingsPatch| apply(&base, &p);

        assert_eq!(
            patch(SettingsPatch {
                language: Some("fr".to_owned()),
                ..Default::default()
            }),
            Err("LANGUAGE")
        );
        assert_eq!(
            patch(SettingsPatch {
                theme: Some("blue".to_owned()),
                ..Default::default()
            }),
            Err("THEME")
        );
        assert_eq!(
            patch(SettingsPatch {
                clipboard_clear_s: Some(5),
                ..Default::default()
            }),
            Err("CLIPBOARD")
        );
        assert_eq!(
            patch(SettingsPatch {
                clipboard_clear_s: Some(121),
                ..Default::default()
            }),
            Err("CLIPBOARD")
        );
        assert!(
            patch(SettingsPatch {
                clipboard_clear_s: Some(120),
                ..Default::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn keeps_fields_it_does_not_know_through_a_change() {
        let mut base = Settings::default();
        base.extra.insert("future".to_owned(), serde_json::json!(1));

        let changed = apply(
            &base,
            &SettingsPatch {
                theme: Some("light".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(changed.extra["future"], 1);
    }

    #[test]
    fn the_panel_opacity_stays_readable() {
        let shown = view(&Settings::default());
        assert_eq!(shown.panel_opacity, 90);

        let at = |opacity| {
            apply(
                &Settings::default(),
                &SettingsPatch {
                    panel_opacity: Some(opacity),
                    ..SettingsPatch::default()
                },
            )
        };

        assert_eq!(at(70).unwrap().panel_opacity, Some(70));
        assert_eq!(at(100).unwrap().panel_opacity, Some(100));
        assert_eq!(at(69), Err("OPACITY"));
        assert_eq!(at(101), Err("OPACITY"));
    }
}

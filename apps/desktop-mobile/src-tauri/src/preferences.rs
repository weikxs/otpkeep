//! The settings page's settings: what the window sees and how a change is checked (design/10-reference.md §2).

use serde::{Deserialize, Serialize};

use crate::settings::{DEFAULT_CLEAR_S, DEFAULT_MENU_ACCOUNTS, Settings};

/// Shortest and longest clipboard clearing time; 0 turns clearing off.
const CLEAR_RANGE: std::ops::RangeInclusive<u16> = 10..=120;
/// Fewest and most accounts on the menu bar menu's first level.
const MENU_ACCOUNTS_RANGE: std::ops::RangeInclusive<u8> = 10..=50;

/// The settings as the window shows them.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent settings the window shows as they are"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// `system`, `zh` or `en`.
    pub language: String,
    /// `system`, `light` or `dark`.
    pub theme: String,
    /// 0 when copied codes are never cleared.
    pub clipboard_clear_s: u16,
    /// How many accounts the menu bar menu shows on its first level.
    pub menu_accounts: u8,
    /// Whether scripts may get codes from the running app.
    pub scripts: bool,
    /// What `scripts` is for this vault when never changed.
    pub scripts_default: bool,
    pub row_shortcuts: bool,
    /// What `row_shortcuts` is on this system when never changed.
    pub row_shortcuts_default: bool,
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
    pub menu_accounts: Option<u8>,
    pub row_shortcuts: Option<bool>,
    pub scripts: Option<bool>,
}

/// The settings as the windows show them; `uses_master_password` decides whether scripts are allowed when the
/// person never chose.
#[must_use]
pub fn view(settings: &Settings, uses_master_password: bool) -> SettingsView {
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
        menu_accounts: settings.menu_accounts.unwrap_or(DEFAULT_MENU_ACCOUNTS),
        scripts: settings.scripts_allowed(uses_master_password),
        scripts_default: !uses_master_password,
        row_shortcuts: settings.row_shortcuts_on(),
        row_shortcuts_default: cfg!(target_os = "macos"),
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
/// Returns the rule that a value breaks: `LANGUAGE`, `THEME`, `CLIPBOARD` or `MENU_ACCOUNTS`.
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
    if let Some(allowed) = patch.scripts {
        next.scripts = Some(allowed);
    }
    if let Some(count) = patch.menu_accounts {
        if !MENU_ACCOUNTS_RANGE.contains(&count) {
            return Err("MENU_ACCOUNTS");
        }
        next.menu_accounts = Some(count);
    }
    if let Some(on) = patch.row_shortcuts {
        next.row_shortcuts = Some(on);
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
        let shown = view(&Settings::default(), false);

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
            menu_accounts: None,
            row_shortcuts: Some(false),
            scripts: None,
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
    fn scripts_follow_the_vault_until_chosen() {
        assert!(view(&Settings::default(), false).scripts);
        let locked = view(&Settings::default(), true);
        assert!(!locked.scripts && !locked.scripts_default);

        let chosen = apply(
            &Settings::default(),
            &SettingsPatch {
                scripts: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(view(&chosen, true).scripts);
    }

    #[test]
    fn the_menu_shows_10_to_50_accounts_on_its_first_level() {
        assert_eq!(view(&Settings::default(), false).menu_accounts, 30);
        let at = |count| {
            apply(
                &Settings::default(),
                &SettingsPatch {
                    menu_accounts: Some(count),
                    ..Default::default()
                },
            )
        };

        assert_eq!(at(10).unwrap().menu_accounts(), 10);
        assert_eq!(at(50).unwrap().menu_accounts(), 50);
        assert_eq!(at(9), Err("MENU_ACCOUNTS"));
        assert_eq!(at(51), Err("MENU_ACCOUNTS"));
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
}

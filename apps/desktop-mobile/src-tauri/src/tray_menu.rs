//! What the menu bar menu lists, worked out from the state of the data alone so it can be tested; drawing it
//! with the system's menu is in [`crate::tray`].
//!
//! Unlocked, the menu starts with Search, then lists the recently used accounts filled up in list order (as many
//! as the setting says), every other account in a "More accounts" submenu (native menus scroll), each with its
//! current code, and the app's actions. Before the master
//! password it offers only to unlock; when the data cannot be opened, what can be done about it.

use crate::core::{EntryKind, MenuView, PanelEntry, PanelState, PanelStatus};

mod words;

use words::{EN, Words, ZH, problem_text, retryable};

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Item(Item),
    Separator,
    /// A submenu: its title and its lines.
    Submenu {
        id: String,
        text: String,
        lines: Vec<Line>,
    },
}

/// A menu item: its id says what choosing it does (see [`Action::of`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub text: String,
    /// The shortcut shown beside it, in canonical form (`control+alt+super+Space`).
    pub accelerator: Option<String>,
    pub enabled: bool,
}

/// What choosing a menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Copies the entry's current code.
    Copy(String),
    /// Makes the counter-based entry's next code and copies it.
    NextCode(String),
    Search,
    Open,
    Manage,
    Sync,
    Settings,
    Unlock,
    Retry,
    /// Opens the main window, where putting back the previous version is confirmed.
    Restore,
    Quit,
}

const COPY: &str = "copy:";
const NEXT_CODE: &str = "next:";
const SECTION: char = '|';

impl Action {
    /// The action of the item with `id`; `None` for lines that only inform.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
        // An account listed in several places has one item in each, told apart by a section before `|`.
        let id = id.split_once(SECTION).map_or(id, |(_, rest)| rest);
        if let Some(entry) = id.strip_prefix(COPY) {
            return Some(Self::Copy(entry.to_owned()));
        }
        if let Some(entry) = id.strip_prefix(NEXT_CODE) {
            return Some(Self::NextCode(entry.to_owned()));
        }
        Some(match id {
            "search" => Self::Search,
            "open" => Self::Open,
            "manage" => Self::Manage,
            "sync" => Self::Sync,
            "settings" => Self::Settings,
            "unlock" => Self::Unlock,
            "retry" => Self::Retry,
            "restore" => Self::Restore,
            "quit" => Self::Quit,
            _ => return None,
        })
    }
}

/// Gap between an account's name and its code.
const GAP: &str = "   ";

/// The menu for `view`. `search_shortcut` is the search panel's shortcut, shown beside its item.
#[must_use]
pub fn lines(view: &MenuView, search_shortcut: Option<&str>, chinese: bool) -> Vec<Line> {
    let words = if chinese { &ZH } else { &EN };
    let state = &view.state;
    match state.status {
        PanelStatus::Ready => ready(view, search_shortcut, words),
        PanelStatus::NeedsPassword => vec![
            item("unlock", words.unlock),
            item("open", words.open),
            Line::Separator,
            item("quit", words.quit),
        ],
        PanelStatus::Unavailable => unavailable(state, chinese, words),
    }
}

fn ready(view: &MenuView, search_shortcut: Option<&str>, words: &Words) -> Vec<Line> {
    let search = Line::Item(Item {
        accelerator: search_shortcut.map(str::to_owned),
        ..plain("search", words.search)
    });
    let mut lines = vec![search, Line::Separator];
    let first = &view.state.entries;
    if first.is_empty() {
        lines.push(informing("none", words.no_accounts));
    } else {
        lines.extend(accounts("a", first, words));
    }
    let more = view.more.len() + view.left_out;
    if more > 0 {
        let mut listed = accounts("m", &view.more, words);
        if view.left_out > 0 {
            listed.push(informing("left-out", &(words.left_out)(view.left_out)));
        }
        lines.extend([
            Line::Separator,
            Line::Submenu {
                id: "more".to_owned(),
                text: (words.more)(more),
                lines: listed,
            },
        ]);
    }
    lines.extend([
        Line::Separator,
        item("open", words.open),
        item("manage", words.manage),
        item("sync", words.sync),
        item("settings", words.settings),
        Line::Separator,
        item("quit", words.quit),
    ]);
    lines
}

/// The accounts of `listed` in section `section`.
fn accounts(section: &str, listed: &[PanelEntry], words: &Words) -> Vec<Line> {
    listed
        .iter()
        .map(|entry| account(section, entry, words))
        .collect()
}

fn unavailable(state: &PanelState, chinese: bool, words: &Words) -> Vec<Line> {
    let mut lines = vec![informing("problem", words.unavailable)];
    if let Some(problem) = state.problem {
        lines.push(informing("problem-detail", problem_text(problem, chinese)));
    }
    lines.push(Line::Separator);
    if state.problem.is_some_and(retryable) {
        lines.push(item("retry", words.retry));
    }
    if state.can_restore {
        lines.push(item("restore", words.restore));
    }
    lines.extend([
        item("open", words.open),
        Line::Separator,
        item("quit", words.quit),
    ]);
    lines
}

/// One account: its name, what tells it apart (its note, or else its account name) and its current code, or
/// what choosing it does when there is no code to show. Only the entry's own shortcut is shown: the default ones
/// follow the list's order, not the menu's.
fn account(section: &str, entry: &PanelEntry, words: &Words) -> Line {
    let (id, code) = match (entry.kind, &entry.code) {
        (EntryKind::Hotp, _) => (
            format!("{NEXT_CODE}{}", entry.id),
            words.press_for_code.to_owned(),
        ),
        (_, Some(code)) => (format!("{COPY}{}", entry.id), group_code(code)),
        (_, None) => (format!("{COPY}{}", entry.id), words.no_code.to_owned()),
    };
    let own = entry
        .shortcut
        .as_ref()
        .filter(|active| entry.own_shortcut.as_ref() == Some(*active));
    Line::Item(Item {
        id: format!("{section}{SECTION}{id}"),
        text: format!("{}{GAP}{code}", escape(&label(entry))),
        accelerator: own.cloned(),
        enabled: true,
    })
}

/// Longest note shown beside a name, in characters.
const NOTE_CHARS: usize = 20;

/// `Name · Note`, `Name · account` without a note, or the name alone.
fn label(entry: &PanelEntry) -> String {
    let note = entry.note.split_whitespace().collect::<Vec<_>>().join(" ");
    let detail = if note.is_empty() {
        entry.account.trim().to_owned()
    } else {
        shorten(&note, NOTE_CHARS)
    };
    if detail.is_empty() {
        entry.title.clone()
    } else {
        format!("{} · {detail}", entry.title)
    }
}

/// `text` cut to `most` characters (not bytes), ending in `…` when cut.
fn shorten(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let kept: String = text.chars().take(most.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Codes in groups, as the windows show them: 3 + 3, 3 + 4 or 4 + 4 digits.
fn group_code(code: &str) -> String {
    let split = match code.len() {
        6 | 7 => 3,
        8 => 4,
        _ => return code.to_owned(),
    };
    if !code.is_char_boundary(split) {
        return code.to_owned();
    }
    let (first, second) = code.split_at(split);
    format!("{first} {second}")
}

/// Menus take `&` as the mark of a mnemonic; a name's own `&` is doubled to show as it is.
fn escape(text: &str) -> String {
    text.replace('&', "&&")
}

/// Whether `new` has the same items as `old` in the same places, so only their words need changing.
#[must_use]
pub fn same_shape(old: &[Line], new: &[Line]) -> bool {
    old.len() == new.len()
        && old.iter().zip(new).all(|pair| match pair {
            (Line::Separator, Line::Separator) => true,
            (Line::Item(old), Line::Item(new)) => {
                old.id == new.id && old.accelerator == new.accelerator && old.enabled == new.enabled
            }
            (
                Line::Submenu {
                    id: old_id,
                    text: old_text,
                    lines: old_lines,
                },
                Line::Submenu {
                    id: new_id,
                    text: new_text,
                    lines: new_lines,
                },
            ) => old_id == new_id && old_text == new_text && same_shape(old_lines, new_lines),
            _ => false,
        })
}

/// Every item, submenus' items included, in the order they are drawn.
#[must_use]
pub fn items(lines: &[Line]) -> Vec<&Item> {
    lines
        .iter()
        .flat_map(|line| match line {
            Line::Item(item) => vec![item],
            Line::Separator => Vec::new(),
            Line::Submenu { lines, .. } => items(lines),
        })
        .collect()
}

/// When the codes shown next change: the earliest end of a shown time-based code.
#[must_use]
pub fn next_change_ms(view: &MenuView) -> Option<u64> {
    view.state
        .entries
        .iter()
        .chain(&view.more)
        .filter(|entry| entry.code.is_some())
        .filter_map(|entry| entry.valid_until_ms)
        .min()
}

fn plain(id: &str, text: &str) -> Item {
    Item {
        id: id.to_owned(),
        text: text.to_owned(),
        accelerator: None,
        enabled: true,
    }
}

fn item(id: &str, text: &str) -> Line {
    Line::Item(plain(id, text))
}

/// A line that only informs: it cannot be chosen.
fn informing(id: &str, text: &str) -> Line {
    Line::Item(Item {
        enabled: false,
        ..plain(id, text)
    })
}

#[cfg(test)]
mod tests;

//! What the menu bar menu lists, worked out from the state of the data alone so it can be tested; drawing it
//! with the system's menu is in [`crate::tray`].
//!
//! Unlocked, the menu lists accounts with their current codes (at most 15) and the app's
//! actions; before the master password it offers only to unlock; when the data cannot be opened, what can be
//! done about it.

use crate::core::{EntryKind, PanelEntry, PanelState, PanelStatus, Problem};

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Item(Item),
    Separator,
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

impl Action {
    /// The action of the item with `id`; `None` for lines that only inform.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
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

/// The menu's words in one language.
struct Words {
    search: &'static str,
    open: &'static str,
    manage: &'static str,
    sync: &'static str,
    settings: &'static str,
    quit: &'static str,
    unlock: &'static str,
    press_for_code: &'static str,
    no_code: &'static str,
    no_accounts: &'static str,
    unavailable: &'static str,
    retry: &'static str,
    restore: &'static str,
}

const ZH: Words = Words {
    search: "搜索全部账号…",
    open: "打开 OTPKeep",
    manage: "在浏览器中管理…",
    sync: "同步…",
    settings: "设置…",
    quit: "退出 OTPKeep",
    unlock: "解锁…",
    press_for_code: "（点击获取验证码）",
    no_code: "（无法计算）",
    no_accounts: "还没有账号",
    unavailable: "暂时无法打开数据",
    retry: "重试",
    restore: "恢复上一个版本…",
};

const EN: Words = Words {
    search: "Search all accounts…",
    open: "Open OTPKeep",
    manage: "Manage in the browser…",
    sync: "Sync…",
    settings: "Settings…",
    quit: "Quit OTPKeep",
    unlock: "Unlock…",
    press_for_code: "(press to get a code)",
    no_code: "(unavailable)",
    no_accounts: "No accounts yet",
    unavailable: "The data cannot be opened right now",
    retry: "Try again",
    restore: "Restore the previous version…",
};

/// Why the data cannot be opened, as the windows say it.
const fn problem_text(problem: Problem, chinese: bool) -> &'static str {
    match (problem, chinese) {
        (Problem::DataMissing, true) => "数据文件不见了。如果移动过它，请放回原处后点“重试”",
        (Problem::DataMissing, false) => {
            "The data file is missing. If you moved it, put it back and choose Try again"
        }
        (Problem::KeyMissing, true) => "本机找不到这份数据的密钥",
        (Problem::KeyMissing, false) => "This device does not have the key for this data",
        (Problem::Damaged, true) => "数据文件已损坏",
        (Problem::Damaged, false) => "The data file is damaged",
        (Problem::NewerFormat, true) => "数据由更新版本的 OTPKeep 写入，请先更新",
        (Problem::NewerFormat, false) => "A newer OTPKeep wrote this data; please update first",
        (Problem::KeystoreUnavailable, true) => "系统钥匙串暂时不可用",
        (Problem::KeystoreUnavailable, false) => "The system keychain is unavailable right now",
        (Problem::Busy, true) => "另一个 OTPKeep 正在使用这些数据",
        (Problem::Busy, false) => "Another OTPKeep is using this data",
        (Problem::Io, true) => "无法读取数据目录",
        (Problem::Io, false) => "The data directory cannot be read",
        (Problem::NoDataDir, true) => "找不到数据目录",
        (Problem::NoDataDir, false) => "The data directory cannot be found",
    }
}

/// Problems that can pass on their own, as in the windows: trying again can help.
const fn retryable(problem: Problem) -> bool {
    matches!(
        problem,
        Problem::Busy | Problem::KeystoreUnavailable | Problem::Io | Problem::DataMissing
    )
}

/// Gap between an account's name and its code.
const GAP: &str = "   ";

/// The menu for `state`. `search_shortcut` is the search panel's shortcut, shown beside its item.
#[must_use]
pub fn lines(state: &PanelState, search_shortcut: Option<&str>, chinese: bool) -> Vec<Line> {
    let words = if chinese { &ZH } else { &EN };
    match state.status {
        PanelStatus::Ready => ready(state, search_shortcut, words),
        PanelStatus::NeedsPassword => vec![
            item("unlock", words.unlock),
            item("open", words.open),
            Line::Separator,
            item("quit", words.quit),
        ],
        PanelStatus::Unavailable => unavailable(state, chinese, words),
    }
}

fn ready(state: &PanelState, search_shortcut: Option<&str>, words: &Words) -> Vec<Line> {
    let accounts: Vec<Line> = if state.entries.is_empty() {
        vec![informing("none", words.no_accounts)]
    } else {
        state
            .entries
            .iter()
            .map(|entry| account(entry, &state.entries, words))
            .collect()
    };
    let search = Line::Item(Item {
        accelerator: search_shortcut.map(str::to_owned),
        ..plain("search", words.search)
    });
    accounts
        .into_iter()
        .chain([
            Line::Separator,
            search,
            item("open", words.open),
            item("manage", words.manage),
            item("sync", words.sync),
            item("settings", words.settings),
            Line::Separator,
            item("quit", words.quit),
        ])
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

/// One account: its name and current code, or what choosing it does when there is no code to show. Only the
/// entry's own shortcut is shown: the default ones follow the list's order, not the menu's.
fn account(entry: &PanelEntry, shown: &[PanelEntry], words: &Words) -> Line {
    let name = name(entry, shown);
    let (id, detail) = match (entry.kind, &entry.code) {
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
        id,
        text: format!("{}{GAP}{detail}", escape(&name)),
        accelerator: own.cloned(),
        enabled: true,
    })
}

/// The entry's name, with its account when another listed entry has the same name.
fn name(entry: &PanelEntry, shown: &[PanelEntry]) -> String {
    let shared = shown
        .iter()
        .any(|other| other.id != entry.id && other.title == entry.title);
    if shared && !entry.account.is_empty() {
        format!("{} ({})", entry.title, entry.account)
    } else {
        entry.title.clone()
    }
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
            _ => false,
        })
}

/// When the codes shown next change: the earliest end of a shown time-based code.
#[must_use]
pub fn next_change_ms(state: &PanelState) -> Option<u64> {
    state
        .entries
        .iter()
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

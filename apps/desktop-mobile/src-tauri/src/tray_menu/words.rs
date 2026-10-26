//! The menu bar menu's words in English and Simplified Chinese.

use crate::core::Problem;

/// The menu's words in one language.
pub(super) struct Words {
    pub(super) search: &'static str,
    pub(super) open: &'static str,
    pub(super) manage: &'static str,
    pub(super) sync: &'static str,
    pub(super) settings: &'static str,
    pub(super) quit: &'static str,
    pub(super) unlock: &'static str,
    pub(super) press_for_code: &'static str,
    pub(super) no_code: &'static str,
    pub(super) no_accounts: &'static str,
    pub(super) unavailable: &'static str,
    pub(super) retry: &'static str,
    pub(super) restore: &'static str,
    pub(super) more: fn(usize) -> String,
    pub(super) left_out: fn(usize) -> String,
}

pub(super) const ZH: Words = Words {
    search: "搜索…",
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
    more: |n| format!("更多账号（{n}）"),
    left_out: |n| format!("另有 {n} 个账号未列出，请用搜索"),
};

pub(super) const EN: Words = Words {
    search: "Search…",
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
    more: |n| format!("More accounts ({n})"),
    left_out: |n| format!("{n} more accounts not listed: use Search"),
};

/// Why the data cannot be opened, as the windows say it.
pub(super) const fn problem_text(problem: Problem, chinese: bool) -> &'static str {
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
pub(super) const fn retryable(problem: Problem) -> bool {
    matches!(
        problem,
        Problem::Busy | Problem::KeystoreUnavailable | Problem::Io | Problem::DataMissing
    )
}

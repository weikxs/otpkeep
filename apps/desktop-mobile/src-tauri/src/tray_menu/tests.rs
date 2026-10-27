use std::time::Instant;

use super::{Action, Line, items, lines, next_change_ms, same_shape};
use crate::core::{EntryKind, MenuView, PanelEntry, PanelState, PanelStatus, Problem};

const SEARCH: &str = "control+alt+super+Space";

fn entry(id: &str, title: &str, code: Option<&str>) -> PanelEntry {
    PanelEntry {
        id: id.to_owned(),
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "demo@example.com".to_owned(),
        note: String::new(),
        tags: Vec::new(),
        kind: EntryKind::Totp,
        pinned: false,
        code: code.map(str::to_owned),
        next_code: None,
        period_s: Some(30),
        valid_until_ms: Some(1_700_000_040_000),
        shortcut: None,
        own_shortcut: None,
    }
}

fn state(status: PanelStatus, entries: Vec<PanelEntry>) -> PanelState {
    PanelState {
        status,
        entries,
        now_ms: 1_700_000_020_000,
        uses_master_password: true,
        problem: None,
        in_group: false,
        read_only: false,
        can_restore: false,
    }
}

fn view(status: PanelStatus, entries: Vec<PanelEntry>) -> MenuView {
    MenuView {
        state: state(status, entries),
        more: Vec::new(),
        left_out: 0,
    }
}

fn ids(lines: &[Line]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| match line {
            Line::Item(item) => item.id.as_str(),
            Line::Separator => "-",
            Line::Submenu { id, .. } => id.as_str(),
        })
        .collect()
}

fn texts(lines: &[Line]) -> Vec<&str> {
    items(lines)
        .into_iter()
        .map(|item| item.text.as_str())
        .collect()
}

#[test]
fn starts_with_search_then_the_accounts_then_the_actions() {
    let shown = view(
        PanelStatus::Ready,
        vec![
            entry("e1", "Example Cloud", Some("123456")),
            entry("e2", "Demo Git", Some("12345678")),
        ],
    );

    let menu = lines(&shown, Some(SEARCH), false);

    assert_eq!(
        ids(&menu),
        [
            "search",
            "-",
            "a|copy:e1",
            "a|copy:e2",
            "-",
            "open",
            "manage",
            "sync",
            "settings",
            "-",
            "quit"
        ]
    );
    let items = items(&menu);
    assert_eq!(items[0].text, "Search…");
    assert_eq!(items[0].accelerator.as_deref(), Some(SEARCH));
    assert_eq!(items[1].text, "123 456   Example Cloud · demo@example.com");
    assert_eq!(items[2].text, "1234 5678   Demo Git · demo@example.com");
    assert_eq!(items[3].text, "Open OTPKeep");
    assert_eq!(items.last().unwrap().text, "Quit OTPKeep");
    assert_eq!(texts(&lines(&shown, None, true))[0], "搜索…");
}

#[test]
fn lists_the_other_accounts_in_a_more_submenu() {
    let mut shown = view(
        PanelStatus::Ready,
        vec![entry("e1", "Example Cloud", Some("123456"))],
    );
    shown.more = vec![
        entry("e2", "Example Mail", Some("654321")),
        entry("e3", "Demo Git", Some("111111")),
    ];

    let en = lines(&shown, None, false);
    let zh = lines(&shown, None, true);

    assert_eq!(ids(&en)[..5], ["search", "-", "a|copy:e1", "-", "more"]);
    let Some(Line::Submenu {
        text, lines: more, ..
    }) = en.get(4)
    else {
        panic!("no submenu");
    };
    assert_eq!(text, "More accounts (2)");
    assert_eq!(ids(more), ["m|copy:e2", "m|copy:e3"]);
    let Some(Line::Submenu { text, .. }) = zh.get(4) else {
        panic!("no submenu");
    };
    assert_eq!(text, "更多账号（2）");
    assert_eq!(Action::of("m|copy:e3"), Some(Action::Copy("e3".to_owned())));
}

#[test]
fn has_no_more_submenu_when_every_account_fits() {
    let menu = lines(
        &view(
            PanelStatus::Ready,
            vec![entry("e1", "Example Cloud", Some("123456"))],
        ),
        None,
        false,
    );

    assert!(!menu.iter().any(|line| matches!(line, Line::Submenu { .. })));
}

#[test]
fn says_how_many_accounts_did_not_fit() {
    let mut shown = view(
        PanelStatus::Ready,
        vec![entry("e1", "Example Cloud", Some("123456"))],
    );
    shown.more = vec![entry("e2", "Example Mail", Some("654321"))];
    shown.left_out = 3;

    let en = lines(&shown, None, false);

    let Some(Line::Submenu { text, .. }) = en.get(4) else {
        panic!("no submenu");
    };
    assert_eq!(text, "More accounts (4)");
    assert!(texts(&en).contains(&"3 more accounts not listed: use Search"));
    assert!(texts(&lines(&shown, None, true)).contains(&"另有 3 个账号未列出，请用搜索"));
}

#[test]
fn puts_the_code_first_so_the_codes_line_up() {
    let mut imported = entry("e1", "Example Cloud:ops@example.com", Some("123456"));
    imported.account = "ops@example.com".to_owned();
    let mut bare = entry("e2", "Demo & Git", None);
    bare.account = String::new();

    let menu = lines(&view(PanelStatus::Ready, vec![imported, bare]), None, false);

    let texts = texts(&menu);
    assert_eq!(texts[1], "123 456   Example Cloud · ops@example.com");
    assert_eq!(texts[2], "(unavailable)   Demo && Git");
}

#[test]
fn shows_only_an_entrys_own_shortcut_that_works() {
    let own = "control+alt+KeyK".to_owned();
    let mut with_own = entry("e1", "Example Cloud", Some("123456"));
    with_own.shortcut = Some(own.clone());
    with_own.own_shortcut = Some(own.clone());
    let mut row_default = entry("e2", "Example Mail", Some("654321"));
    row_default.shortcut = Some("control+alt+super+Digit2".to_owned());
    let mut refused = entry("e3", "Example Bank", Some("111111"));
    refused.own_shortcut = Some("control+alt+KeyJ".to_owned());

    let menu = lines(
        &view(PanelStatus::Ready, vec![with_own, row_default, refused]),
        None,
        false,
    );

    let items = items(&menu);
    assert_eq!(items[0].accelerator, None, "no search shortcut");
    assert_eq!(items[1].accelerator.as_deref(), Some(own.as_str()));
    assert_eq!(
        items[2].accelerator, None,
        "row defaults follow the list, not the menu"
    );
    assert_eq!(items[3].accelerator, None, "the system refused it");
}

#[test]
fn a_counter_entry_makes_its_code_when_chosen() {
    let mut counter = entry("h1", "Example Bank", None);
    counter.kind = EntryKind::Hotp;
    counter.period_s = None;
    counter.valid_until_ms = None;
    counter.account = String::new();

    let en = lines(
        &view(PanelStatus::Ready, vec![counter.clone()]),
        None,
        false,
    );
    let zh = lines(&view(PanelStatus::Ready, vec![counter]), None, true);

    assert_eq!(items(&en)[1].id, "a|next:h1");
    assert_eq!(items(&en)[1].text, "(press for code)   Example Bank");
    assert_eq!(items(&zh)[1].text, "（点击取码）   Example Bank");
}

#[test]
fn an_empty_vault_says_so() {
    let menu = lines(&view(PanelStatus::Ready, Vec::new()), None, true);

    let items = items(&menu);
    assert_eq!(items[1].text, "还没有账号");
    assert!(!items[1].enabled);
}

#[test]
fn before_the_master_password_only_offers_to_unlock() {
    let menu = lines(
        &view(PanelStatus::NeedsPassword, Vec::new()),
        Some(SEARCH),
        false,
    );

    assert_eq!(ids(&menu), ["unlock", "open", "-", "quit"]);
    assert_eq!(texts(&menu)[0], "Unlock…");
    let zh = lines(&view(PanelStatus::NeedsPassword, Vec::new()), None, true);
    assert_eq!(texts(&zh)[0], "解锁…");
    assert_eq!(texts(&zh)[2], "退出 OTPKeep");
}

#[test]
fn data_that_cannot_be_opened_offers_what_can_help() {
    let mut busy = view(PanelStatus::Unavailable, Vec::new());
    busy.state.problem = Some(Problem::Busy);
    let mut damaged = view(PanelStatus::Unavailable, Vec::new());
    damaged.state.problem = Some(Problem::Damaged);
    damaged.state.can_restore = true;

    let busy = lines(&busy, None, false);
    let damaged = lines(&damaged, None, true);

    assert_eq!(
        ids(&busy),
        [
            "problem",
            "problem-detail",
            "-",
            "retry",
            "open",
            "-",
            "quit"
        ]
    );
    assert_eq!(texts(&busy)[1], "Another OTPKeep is using this data");
    assert!(!items(&busy)[1].enabled);
    assert_eq!(
        ids(&damaged),
        [
            "problem",
            "problem-detail",
            "-",
            "restore",
            "open",
            "-",
            "quit"
        ]
    );
    assert_eq!(texts(&damaged)[1], "数据文件已损坏");
}

#[test]
fn every_line_is_worded_in_both_languages() {
    let mut problems = view(PanelStatus::Unavailable, Vec::new());
    problems.state.problem = Some(Problem::DataMissing);
    problems.state.can_restore = true;
    let mut ready = view(
        PanelStatus::Ready,
        vec![entry("e1", "Example Cloud", Some("1"))],
    );
    ready.more = vec![entry("e2", "Example Mail", Some("2"))];
    ready.left_out = 2;
    for shown in [
        view(PanelStatus::Ready, Vec::new()),
        view(PanelStatus::NeedsPassword, Vec::new()),
        problems,
        ready,
    ] {
        let en = lines(&shown, None, false);
        let zh = lines(&shown, None, true);
        for (en, zh) in items(&en).into_iter().zip(items(&zh)) {
            assert_eq!(en.id, zh.id);
            let account = en.id.contains("copy:");
            assert!(account || en.text != zh.text, "{}", en.id);
        }
    }
}

#[test]
fn each_item_says_what_it_does() {
    assert_eq!(Action::of("a|copy:e1"), Some(Action::Copy("e1".to_owned())));
    assert_eq!(
        Action::of("m|next:h1"),
        Some(Action::NextCode("h1".to_owned()))
    );
    assert_eq!(Action::of("search"), Some(Action::Search));
    assert_eq!(Action::of("unlock"), Some(Action::Unlock));
    assert_eq!(Action::of("restore"), Some(Action::Restore));
    assert_eq!(Action::of("quit"), Some(Action::Quit));
    assert_eq!(Action::of("problem"), None);
    assert_eq!(Action::of("more"), None);
    assert_eq!(Action::of("left-out"), None);
}

#[test]
fn the_codes_change_when_the_first_shown_one_ends() {
    let mut later = entry("e2", "Example Mail", Some("654321"));
    later.period_s = Some(60);
    later.valid_until_ms = Some(1_700_000_060_000);
    let mut counter = entry("h1", "Example Bank", None);
    counter.valid_until_ms = None;
    let mut shown = view(PanelStatus::Ready, vec![later, counter]);
    shown.more = vec![entry("e1", "Example Cloud", Some("123456"))];

    assert_eq!(next_change_ms(&shown), Some(1_700_000_040_000));
    assert_eq!(
        next_change_ms(&view(PanelStatus::NeedsPassword, Vec::new())),
        None
    );
}

#[test]
fn new_codes_only_reword_the_menu() {
    let menu = |id: &str, code: &str, search: Option<&str>| {
        let mut shown = view(
            PanelStatus::Ready,
            vec![entry("e0", "Demo Git", Some(code))],
        );
        shown.more = vec![entry(id, "Example Cloud", Some(code))];
        lines(&shown, search, false)
    };
    let before = menu("e1", "123456", None);

    assert!(same_shape(&before, &menu("e1", "654321", None)));
    assert!(!same_shape(&before, &menu("e2", "654321", None)));
    assert!(!same_shape(&before, &menu("e1", "123456", Some(SEARCH))));
    let locked = lines(&view(PanelStatus::NeedsPassword, Vec::new()), None, false);
    assert!(!same_shape(&locked, &before));
}

#[test]
fn a_thousand_accounts_are_laid_out_quickly() {
    let all: Vec<PanelEntry> = (0..1030)
        .map(|n| {
            entry(
                &format!("e{n}"),
                &format!("Example Cloud {n}"),
                Some("123456"),
            )
        })
        .collect();
    let shown = MenuView {
        state: state(PanelStatus::Ready, all[..30].to_vec()),
        more: all[30..].to_vec(),
        left_out: 0,
    };

    let started = Instant::now();
    let menu = lines(&shown, Some(SEARCH), false);
    let again = lines(&shown, Some(SEARCH), false);
    let same = same_shape(&menu, &again);
    let spent = started.elapsed();

    assert!(same);
    assert_eq!(items(&menu).len(), 1 + 1030 + 5);
    // Twice the layout and one comparison take milliseconds; the bound is generous for slow machines.
    assert!(spent.as_millis() < 500, "{spent:?}");
}

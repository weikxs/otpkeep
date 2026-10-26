use super::{Action, Item, Line, lines, next_change_ms, same_shape};
use crate::core::{EntryKind, PanelEntry, PanelState, PanelStatus, Problem};

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

fn items(lines: &[Line]) -> Vec<&Item> {
    lines
        .iter()
        .filter_map(|line| match line {
            Line::Item(item) => Some(item),
            Line::Separator => None,
        })
        .collect()
}

fn ids(lines: &[Line]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| match line {
            Line::Item(item) => item.id.as_str(),
            Line::Separator => "-",
        })
        .collect()
}

#[test]
fn lists_accounts_with_their_codes_then_the_actions() {
    let shown = state(
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
            "copy:e1", "copy:e2", "-", "search", "open", "manage", "sync", "settings", "-", "quit"
        ]
    );
    let items = items(&menu);
    assert_eq!(items[0].text, "Example Cloud   123 456");
    assert_eq!(items[1].text, "Demo Git   1234 5678");
    assert_eq!(items[2].text, "Search all accounts…");
    assert_eq!(items[2].accelerator.as_deref(), Some(SEARCH));
    assert_eq!(items[3].text, "Open OTPKeep");
    assert_eq!(items.last().unwrap().text, "Quit OTPKeep");
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
        &state(PanelStatus::Ready, vec![with_own, row_default, refused]),
        None,
        false,
    );

    let items = items(&menu);
    assert_eq!(items[0].accelerator.as_deref(), Some(own.as_str()));
    assert_eq!(
        items[1].accelerator, None,
        "row defaults follow the list, not the menu"
    );
    assert_eq!(items[2].accelerator, None, "the system refused it");
    assert_eq!(items[3].accelerator, None, "no search shortcut");
}

#[test]
fn a_counter_entry_makes_its_code_when_chosen() {
    let mut counter = entry("h1", "Example Bank", None);
    counter.kind = EntryKind::Hotp;
    counter.period_s = None;
    counter.valid_until_ms = None;

    let en = lines(
        &state(PanelStatus::Ready, vec![counter.clone()]),
        None,
        false,
    );
    let zh = lines(&state(PanelStatus::Ready, vec![counter]), None, true);

    assert_eq!(items(&en)[0].id, "next:h1");
    assert_eq!(items(&en)[0].text, "Example Bank   (press to get a code)");
    assert_eq!(items(&zh)[0].text, "Example Bank   （点击获取验证码）");
}

#[test]
fn tells_apart_accounts_with_the_same_name_and_keeps_ampersands() {
    let mut second = entry("e2", "Example Cloud", Some("654321"));
    second.account = "ops@example.com".to_owned();
    let shown = state(
        PanelStatus::Ready,
        vec![
            entry("e1", "Example Cloud", Some("123456")),
            second,
            entry("e3", "Demo & Git", None),
        ],
    );

    let menu = lines(&shown, None, false);

    let items = items(&menu);
    assert_eq!(items[0].text, "Example Cloud (demo@example.com)   123 456");
    assert_eq!(items[1].text, "Example Cloud (ops@example.com)   654 321");
    assert_eq!(items[2].text, "Demo && Git   (unavailable)");
}

#[test]
fn an_empty_vault_says_so() {
    let menu = lines(&state(PanelStatus::Ready, Vec::new()), None, true);

    let items = items(&menu);
    assert_eq!(items[0].text, "还没有账号");
    assert!(!items[0].enabled);
    assert_eq!(items[1].text, "搜索全部账号…");
}

#[test]
fn before_the_master_password_only_offers_to_unlock() {
    let menu = lines(
        &state(PanelStatus::NeedsPassword, Vec::new()),
        Some(SEARCH),
        false,
    );

    assert_eq!(ids(&menu), ["unlock", "open", "-", "quit"]);
    assert_eq!(items(&menu)[0].text, "Unlock…");
    let zh = lines(&state(PanelStatus::NeedsPassword, Vec::new()), None, true);
    assert_eq!(items(&zh)[0].text, "解锁…");
    assert_eq!(items(&zh)[2].text, "退出 OTPKeep");
}

#[test]
fn data_that_cannot_be_opened_offers_what_can_help() {
    let mut busy = state(PanelStatus::Unavailable, Vec::new());
    busy.problem = Some(Problem::Busy);
    let mut damaged = state(PanelStatus::Unavailable, Vec::new());
    damaged.problem = Some(Problem::Damaged);
    damaged.can_restore = true;

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
    assert_eq!(items(&busy)[1].text, "Another OTPKeep is using this data");
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
    assert_eq!(items(&damaged)[1].text, "数据文件已损坏");
}

#[test]
fn every_line_is_worded_in_both_languages() {
    let mut problems = state(PanelStatus::Unavailable, Vec::new());
    problems.problem = Some(Problem::DataMissing);
    problems.can_restore = true;
    for shown in [
        state(PanelStatus::Ready, Vec::new()),
        state(PanelStatus::NeedsPassword, Vec::new()),
        problems,
    ] {
        let en = lines(&shown, None, false);
        let zh = lines(&shown, None, true);
        for (en, zh) in items(&en).into_iter().zip(items(&zh)) {
            assert_eq!(en.id, zh.id);
            assert_ne!(en.text, zh.text, "{}", en.id);
        }
    }
}

#[test]
fn each_item_says_what_it_does() {
    assert_eq!(Action::of("copy:e1"), Some(Action::Copy("e1".to_owned())));
    assert_eq!(
        Action::of("next:h1"),
        Some(Action::NextCode("h1".to_owned()))
    );
    assert_eq!(Action::of("search"), Some(Action::Search));
    assert_eq!(Action::of("unlock"), Some(Action::Unlock));
    assert_eq!(Action::of("restore"), Some(Action::Restore));
    assert_eq!(Action::of("quit"), Some(Action::Quit));
    assert_eq!(Action::of("problem"), None);
    assert_eq!(Action::of("none"), None);
}

#[test]
fn the_codes_change_when_the_first_shown_one_ends() {
    let mut later = entry("e2", "Example Mail", Some("654321"));
    later.period_s = Some(60);
    later.valid_until_ms = Some(1_700_000_060_000);
    let mut counter = entry("h1", "Example Bank", None);
    counter.valid_until_ms = None;
    let shown = state(
        PanelStatus::Ready,
        vec![later, entry("e1", "Example Cloud", Some("123456")), counter],
    );

    assert_eq!(next_change_ms(&shown), Some(1_700_000_040_000));
    assert_eq!(
        next_change_ms(&state(PanelStatus::NeedsPassword, Vec::new())),
        None
    );
}

#[test]
fn new_codes_only_reword_the_menu() {
    let before = lines(
        &state(
            PanelStatus::Ready,
            vec![entry("e1", "Example Cloud", Some("123456"))],
        ),
        None,
        false,
    );
    let new_code = lines(
        &state(
            PanelStatus::Ready,
            vec![entry("e1", "Example Cloud", Some("654321"))],
        ),
        None,
        false,
    );
    let other_entry = lines(
        &state(
            PanelStatus::Ready,
            vec![entry("e2", "Example Cloud", Some("654321"))],
        ),
        None,
        false,
    );
    let unlocked = lines(&state(PanelStatus::NeedsPassword, Vec::new()), None, false);

    assert!(same_shape(&before, &new_code));
    assert!(!same_shape(&before, &other_entry));
    assert!(!same_shape(&unlocked, &before));
    assert!(!same_shape(
        &before,
        &lines(
            &state(
                PanelStatus::Ready,
                vec![entry("e1", "Example Cloud", Some("123456"))]
            ),
            Some(SEARCH),
            false
        )
    ));
}

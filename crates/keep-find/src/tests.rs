use std::collections::HashSet;

use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial};
use proptest::prelude::*;

use super::{FieldKind, FindContext, FindHit, FindIndex};

fn stamp() -> Hlc {
    Hlc {
        wall_ms: 1,
        logical: 0,
        device: DeviceTag([0; 8]),
    }
}

fn field<T>(value: T) -> Field<T> {
    Field {
        value,
        stamp: stamp(),
    }
}

fn entry(n: u8, title: &str, issuer: &str, account: &str, note: &str, tags: &[&str]) -> Entry {
    Entry {
        id: EntryId([n; 16]),
        created_at: stamp(),
        title: field(title.to_owned()),
        issuer: field(issuer.to_owned()),
        account: field(account.to_owned()),
        key: field(KeyMaterial {
            secret: SecretKey::from_bytes(vec![n; 20]).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            key_id: stamp(),
        }),
        note: field(note.to_owned()),
        tags: field(tags.iter().map(|t| (*t).to_owned()).collect()),
        rank: field(format!("{n:03}")),
        pinned: field(false),
        keep_duplicate: field(false),
        trashed_at: field(None),
        old_key_of: field(None),
        extra: Extra::new(),
    }
}

fn titled(n: u8, title: &str) -> Entry {
    entry(n, title, "", "", "", &[])
}

fn find(entries: &[Entry], query: &str) -> Vec<FindHit> {
    let refs: Vec<&Entry> = entries.iter().collect();
    FindIndex::build(&refs).find(query, &FindContext::default())
}

fn found_titles(entries: &[Entry], query: &str) -> Vec<String> {
    let hits = find(entries, query);
    hits.iter()
        .map(|hit| {
            entries
                .iter()
                .find(|e| e.id == hit.entry)
                .unwrap()
                .title
                .value
                .clone()
        })
        .collect()
}

fn ranges(hit: &FindHit, kind: FieldKind) -> Vec<std::ops::Range<usize>> {
    hit.matched
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, r)| r.clone())
        .unwrap_or_default()
}

#[test]
fn ignores_case_width_and_accents() {
    let entries = [
        titled(1, "ＧｉｔＨｕｂ"),
        titled(2, "Café Rouge"),
        titled(3, "Other"),
    ];

    assert_eq!(found_titles(&entries, "github"), ["ＧｉｔＨｕｂ"]);
    assert_eq!(found_titles(&entries, "CAFE"), ["Café Rouge"]);
    assert_eq!(found_titles(&entries, "café   rouge"), ["Café Rouge"]);
}

#[test]
fn ranks_exact_then_prefix_then_word_then_substring_then_fuzzy() {
    let entries = [
        titled(1, "Gaxiyt"),
        titled(2, "ProdGitHub"),
        titled(3, "My GitHub"),
        titled(4, "GitHub Work"),
        titled(5, "git"),
        titled(6, "Unrelated"),
    ];

    assert_eq!(
        found_titles(&entries, "git"),
        ["git", "GitHub Work", "My GitHub", "ProdGitHub", "Gaxiyt"]
    );
}

#[test]
fn finds_chinese_by_pinyin_initials_and_full_spelling() {
    let entries = [titled(1, "支付宝"), titled(2, "微信")];

    for query in ["zfb", "zhifubao", "zhifu", "zhif", "支付", "宝"] {
        assert_eq!(found_titles(&entries, query), ["支付宝"], "{query}");
    }
}

#[test]
fn every_reading_of_a_character_counts() {
    let entries = [titled(1, "银行"), titled(2, "重庆"), titled(3, "绿洲")];

    for (query, title) in [
        ("yinhang", "银行"),
        ("yh", "银行"),
        ("yx", "银行"),
        ("chongqing", "重庆"),
        ("zhongqing", "重庆"),
        ("cq", "重庆"),
        ("lvzhou", "绿洲"),
        ("luzhou", "绿洲"),
    ] {
        assert_eq!(found_titles(&entries, query), [title], "{query}");
    }
}

#[test]
fn pinyin_spans_mixed_text_and_can_start_mid_name() {
    let entries = [titled(1, "支付宝 App"), titled(2, "我的银行卡")];

    assert_eq!(found_titles(&entries, "zfbapp"), ["支付宝 App"]);
    assert_eq!(found_titles(&entries, "yhk"), ["我的银行卡"]);
}

#[test]
fn every_word_must_match_somewhere() {
    let entries = [
        entry(1, "GitHub", "", "", "work account", &[]),
        titled(2, "GitHub"),
    ];

    assert_eq!(found_titles(&entries, "github work"), ["GitHub"]);
    assert_eq!(find(&entries, "github work")[0].entry, EntryId([1; 16]));
    assert!(find(&entries, "github nothing").is_empty());
}

#[test]
fn names_count_more_than_notes() {
    let entries = [entry(1, "Other", "", "", "demo", &[]), titled(2, "Demo")];

    assert_eq!(found_titles(&entries, "demo"), ["Demo", "Other"]);
}

#[test]
fn searches_issuer_account_tags_and_notes() {
    let entries = [entry(
        1,
        "A",
        "Example Cloud",
        "alex@example.com",
        "备注里的关键词",
        &["公司"],
    )];

    for query in ["example", "alex@", "gongsi", "公司", "关键词", "gjc"] {
        assert_eq!(find(&entries, query).len(), 1, "{query}");
    }
}

#[test]
fn highlights_the_matched_characters() {
    let cases = [
        ("My GitHub", "git", 3..6),
        ("支付宝", "zfb", 0..3),
        ("ＧｉｔＨｕｂ", "hub", 3..6),
    ];

    for (title, query, expected) in cases {
        let hit = &find(&[titled(1, title)], query)[0];
        assert_eq!(
            ranges(hit, FieldKind::Title),
            [expected],
            "{title} / {query}"
        );
    }
}

#[test]
fn a_note_hit_shows_a_snippet_around_it() {
    let note = format!("{} 恢复码在抽屉里 {}", "前".repeat(40), "后".repeat(40));
    let entries = [entry(1, "A", "", "", &note, &[])];

    let snippet = find(&entries, "抽屉")[0].note_snippet.clone().unwrap();

    assert!(snippet.contains("抽屉"), "{snippet}");
    assert!(
        snippet.starts_with('…') && snippet.ends_with('…'),
        "{snippet}"
    );
    assert!(snippet.chars().count() <= 50, "{snippet}");
}

#[test]
fn recently_used_and_pinned_entries_win_ties() {
    let mut pinned = titled(2, "Demo B");
    pinned.pinned = field(true);
    let entries = [titled(1, "Demo A"), pinned, titled(3, "Demo C")];
    let refs: Vec<&Entry> = entries.iter().collect();
    let recent: HashSet<EntryId> = [EntryId([3; 16])].into_iter().collect();

    let hits = FindIndex::build(&refs).find(
        "demo",
        &FindContext {
            recently_used: recent,
        },
    );
    let order: Vec<EntryId> = hits.iter().map(|h| h.entry).collect();

    assert_eq!(
        order,
        [EntryId([3; 16]), EntryId([2; 16]), EntryId([1; 16])]
    );
}

#[test]
fn an_empty_query_lists_everything_in_order() {
    let entries = [titled(1, "b"), titled(2, "a")];

    assert_eq!(found_titles(&entries, "   "), ["b", "a"]);
}

#[test]
fn equal_scores_keep_the_given_order() {
    let entries = [titled(1, "Demo 2"), titled(2, "Demo 1")];

    assert_eq!(found_titles(&entries, "demo"), ["Demo 2", "Demo 1"]);
}

proptest! {
    #[test]
    fn never_panics_and_highlights_stay_inside_the_field(
        title in "\\PC{0,30}",
        note in "\\PC{0,60}",
        query in "\\PC{0,12}",
    ) {
        let entries = [entry(1, &title, "", "", &note, &[])];
        for hit in find(&entries, &query) {
            for (kind, ranges) in &hit.matched {
                let len = match kind {
                    FieldKind::Title => title.chars().count(),
                    FieldKind::Note => note.chars().count(),
                    _ => 0,
                };
                for r in ranges {
                    prop_assert!(r.start < r.end && r.end <= len, "{r:?} in {len}");
                }
            }
        }
    }

    #[test]
    fn a_name_always_finds_itself(title in "[a-zA-Z\\u{4e00}-\\u{4fff} ]{1,20}") {
        prop_assume!(!title.trim().is_empty());
        let entries = [titled(1, &title)];

        prop_assert_eq!(find(&entries, &title).len(), 1);
    }
}

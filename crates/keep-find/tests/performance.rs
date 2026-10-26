//! PRD FR-3.10: with 1000 entries, one keystroke's search finishes within 16 ms. Measured on release builds
//! only (`cargo test --release`); debug builds are not representative.
//!
//! The app keeps the index between keystrokes and builds it again only after the entries change, so a keystroke
//! costs one search; the first keystroke after a change also pays for the build, which must fit the same budget.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "a test harness that reports its timings"
)]

use std::time::{Duration, Instant};

use keep_find::{FindContext, FindIndex};
use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial};

const ENTRIES: usize = 1000;
const BUDGET: Duration = Duration::from_millis(16);
const RUNS: usize = 25;

fn field<T>(value: T) -> Field<T> {
    Field {
        value,
        stamp: Hlc {
            wall_ms: 1,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
    }
}

fn demo_entry(i: usize) -> Entry {
    let names = [
        "Example Cloud",
        "示例银行",
        "Demo Git",
        "支付演示",
        "Mail Demo",
        "云服务演示",
    ];
    let name = names[i % names.len()];
    let note = format!(
        "{name} 的备注 {i}：恢复码放在抽屉里，联系人 demo-user，账号 alex{i}@example.com。"
    )
    .repeat(3);
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&(i as u64).to_be_bytes());
    Entry {
        id: EntryId(id),
        created_at: field(()).stamp,
        title: field(format!("{name} {i}")),
        issuer: field(name.to_owned()),
        account: field(format!("alex{i}@example.com")),
        key: field(KeyMaterial {
            secret: SecretKey::from_bytes(vec![7; 20]).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            key_id: field(()).stamp,
        }),
        note: field(note.chars().take(200).collect()),
        tags: field(vec!["work".to_owned(), "演示".to_owned()]),
        rank: field(format!("{i:04}")),
        pinned: field(false),
        keep_duplicate: field(false),
        trashed_at: field(None),
        old_key_of: field(None),
        extra: Extra::new(),
    }
}

/// The median of `RUNS` timings of `f`.
fn median(mut f: impl FnMut()) -> Duration {
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    times.sort();
    times[RUNS / 2]
}

#[cfg_attr(
    debug_assertions,
    ignore = "timing is only meaningful in release builds"
)]
#[test]
fn one_keystroke_searches_1000_entries_within_16_ms() {
    let entries: Vec<Entry> = (0..ENTRIES).map(demo_entry).collect();
    let refs: Vec<&Entry> = entries.iter().collect();
    let index = FindIndex::build(&refs);
    let context = FindContext::default();

    for query in [
        "e",
        "exa",
        "yh",
        "shili",
        "抽屉",
        "alex999",
        "demo work",
        "zzzz-no-match",
    ] {
        let took = median(|| {
            let _ = index.find(query, &context);
        });
        println!("{query:>14}: {took:?}");
        assert!(took <= BUDGET, "'{query}' took {took:?}");
    }
}

#[cfg_attr(
    debug_assertions,
    ignore = "timing is only meaningful in release builds"
)]
#[test]
fn the_first_keystroke_after_a_change_builds_and_searches_1000_entries_within_16_ms() {
    let entries: Vec<Entry> = (0..ENTRIES).map(demo_entry).collect();
    let refs: Vec<&Entry> = entries.iter().collect();
    let context = FindContext::default();

    let build = median(|| {
        let _ = FindIndex::build(&refs);
    });
    println!("{:>14}: {build:?}", "build");
    for query in ["e", "shili", "zzzz-no-match"] {
        let took = median(|| {
            let _ = FindIndex::build(&refs).find(query, &context);
        });
        println!("{:>14}: {took:?}", format!("build+{query}"));
        assert!(
            took <= BUDGET,
            "building and finding '{query}' took {took:?}"
        );
    }
}

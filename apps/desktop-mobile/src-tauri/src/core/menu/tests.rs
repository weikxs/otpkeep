use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeSpec, SecretKey};
use keep_platform::MemoryKeystore;
use keep_vault::edit::{Change, NewEntry};
use keep_vault::open::{create_vault, create_with_master_password};
use keep_vault::sealed::KdfParams;
use keep_vault::service::VaultService;
use keep_vault::store::VaultPaths;

use super::{MENU_LIMIT, with_used};
use crate::core::{Core, PanelStatus};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-menu-")
        .tempdir_in(std::env::temp_dir())
        .unwrap()
}

fn demo(index: u8) -> NewEntry {
    NewEntry {
        title: format!("Example Cloud {index:02}"),
        issuer: "Example Cloud".to_owned(),
        account: "demo@example.com".to_owned(),
        secret: SecretKey::from_bytes(vec![index; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: Vec::new(),
        keep_duplicate: true,
    }
}

/// A vault with `count` entries named `Example Cloud 00` …, the ones at `pinned` pinned.
fn core_with(dir: &Path, count: u8, pinned: &[u8]) -> Core {
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir), &keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || NOW_MS);
    for index in 0..count {
        let id = service.add(demo(index)).unwrap();
        if pinned.contains(&index) {
            service.edit(id, Change::Pinned(true)).unwrap();
        }
    }
    drop(service);
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(keystore),
        Arc::new(|| NOW_MS),
        WAIT,
    )
}

fn titles(core: &Core, recent: &[String]) -> Vec<String> {
    core.menu_state(recent)
        .entries
        .into_iter()
        .map(|entry| entry.title.replace("Example Cloud ", ""))
        .collect()
}

fn id_of(core: &Core, title: &str) -> String {
    core.state_all("")
        .entries
        .into_iter()
        .find(|entry| entry.title == title)
        .unwrap()
        .id
}

#[test]
fn lists_pinned_accounts_first_then_the_recently_used_then_the_rest() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 5, &[3]);
    let recent = vec![
        id_of(&core, "Example Cloud 04"),
        id_of(&core, "Example Cloud 01"),
        // A pinned one used recently stays with the pinned ones.
        id_of(&core, "Example Cloud 03"),
    ];

    assert_eq!(titles(&core, &recent), ["03", "04", "01", "00", "02"]);
}

#[test]
fn lists_at_most_the_menu_limit_with_codes() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 20, &[]);
    let last = id_of(&core, "Example Cloud 19");

    let state = core.menu_state(std::slice::from_ref(&last));

    assert_eq!(state.status, PanelStatus::Ready);
    assert_eq!(state.entries.len(), MENU_LIMIT);
    assert_eq!(state.entries[0].id, last, "a recent one far down is listed");
    assert!(state.entries.iter().all(|entry| entry.code.is_some()));
    assert_eq!(state.entries[0].valid_until_ms, Some(1_700_000_040_000));
}

#[test]
fn ignores_recent_entries_that_are_gone() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 2, &[]);

    assert_eq!(titles(&core, &["gone".to_owned()]), ["00", "01"]);
}

#[test]
fn lists_nothing_before_the_master_password() {
    let dir = temp_dir();
    let params = KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let open =
        create_with_master_password(&VaultPaths::new(dir.path()), "demo password", params, WAIT)
            .unwrap();
    drop(open);
    let core = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );

    let state = core.menu_state(&[]);

    assert_eq!(state.status, PanelStatus::NeedsPassword);
    assert!(state.entries.is_empty());
}

#[test]
fn remembers_each_used_entry_once_most_recent_first() {
    let recent = with_used(&[], "a");
    let recent = with_used(&recent, "b");
    let recent = with_used(&recent, "a");

    assert_eq!(recent, ["a", "b"]);
}

#[test]
fn remembers_a_bounded_number_of_entries() {
    let many = (0..80).fold(Vec::new(), |recent, n| with_used(&recent, &n.to_string()));

    assert_eq!(many.len(), 50);
    assert_eq!(many[0], "79");
}

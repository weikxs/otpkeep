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

use super::MENU_LIMIT;
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

fn short(entries: &[crate::core::PanelEntry]) -> Vec<String> {
    entries
        .iter()
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
fn lists_the_recently_used_first_then_fills_up_in_list_order() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 6, &[3]);
    let recent = vec![
        id_of(&core, "Example Cloud 05"),
        "gone".to_owned(),
        id_of(&core, "Example Cloud 01"),
    ];

    let view = core.menu_view(&recent, 4);

    assert_eq!(view.state.status, PanelStatus::Ready);
    assert_eq!(short(&view.state.entries), ["05", "01", "03", "00"]);
    assert_eq!(short(&view.more), ["02", "04"]);
    assert!(view.state.entries.iter().all(|entry| entry.code.is_some()));
    assert_eq!(
        view.state.entries[0].valid_until_ms,
        Some(1_700_000_040_000)
    );
    assert_eq!(view.left_out, 0);
}

#[test]
fn has_nothing_more_when_every_account_fits() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 3, &[]);

    let view = core.menu_view(&[], 30);

    assert_eq!(short(&view.state.entries), ["00", "01", "02"]);
    assert!(view.more.is_empty());
}

#[test]
fn lists_every_other_account_however_many() {
    let dir = temp_dir();
    let core = core_with(dir.path(), 40, &[]);

    let view = core.menu_view(&[], 10);

    assert_eq!(view.state.entries.len(), 10);
    assert_eq!(view.more.len(), 30);
    const { assert!(MENU_LIMIT >= 1000) };
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

    let view = core.menu_view(&["e1".to_owned()], 30);

    assert_eq!(view.state.status, PanelStatus::NeedsPassword);
    assert!(view.state.entries.is_empty());
    assert!(view.more.is_empty());
}

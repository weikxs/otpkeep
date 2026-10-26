use std::sync::Arc;
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::store::VaultPaths;

use crate::core::{Core, EntryUpdate, Vault};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-search-")
        .tempdir()
        .unwrap()
}

fn add(app: &mut Core, links: &str) {
    let view = app.stage_text(links).unwrap();
    let all = Choice {
        fresh: vec![true; view.rows.len()],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &all).unwrap();
}

fn with_two(dir: &std::path::Path) -> Core {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );
    add(
        &mut app,
        "otpauth://totp/Example%20Cloud:alex@example.com?secret=JBSWY3DPEHPK3PXP\n\
         otpauth://totp/Example%20Bank:demo-user?secret=GEZDGNBVGY3TQOJQ",
    );
    app
}

/// The revision the search index was built from, if one was built.
fn index_built_from(app: &Core) -> Option<u64> {
    match &app.vault {
        Vault::Open { find, .. } => find.built_from(),
        _ => None,
    }
}

fn found(app: &Core, query: &str) -> Vec<String> {
    app.state_all(query)
        .entries
        .into_iter()
        .map(|e| e.title)
        .collect()
}

#[test]
fn keystrokes_reuse_the_index_until_the_vault_changes() {
    let dir = temp_dir();
    let app = with_two(dir.path());
    assert_eq!(index_built_from(&app), None);

    let first = found(&app, "ex");
    let built = index_built_from(&app);
    let second = found(&app, "exa bank");

    assert_eq!(first.len(), 2);
    assert_eq!(second, ["Example Bank"]);
    assert_eq!(built, app.revision());
    assert_eq!(index_built_from(&app), built);
}

#[test]
fn a_search_after_an_edit_sees_the_edit() {
    let dir = temp_dir();
    let mut app = with_two(dir.path());
    assert_eq!(found(&app, "demo git"), Vec::<String>::new());
    let id = app.list("bank").unwrap().remove(0).id;

    let update = EntryUpdate {
        title: Some("Demo Git".to_owned()),
        ..EntryUpdate::default()
    };
    app.update(&id, update).unwrap();

    assert_eq!(found(&app, "demo git"), ["Demo Git"]);
    assert_eq!(index_built_from(&app), app.revision());
}

#[test]
fn a_search_after_an_import_or_the_trash_sees_the_change() {
    let dir = temp_dir();
    let mut app = with_two(dir.path());
    assert_eq!(found(&app, "mail").len(), 0);

    add(
        &mut app,
        "otpauth://totp/Example%20Mail:ops@example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBV",
    );
    assert_eq!(found(&app, "mail"), ["Example Mail"]);

    let id = app.list("mail").unwrap().remove(0).id;
    app.trash(&id).unwrap();
    assert_eq!(found(&app, "mail").len(), 0);
}

#[test]
fn places_follow_the_whole_list_while_searching() {
    let dir = temp_dir();
    let app = with_two(dir.path());

    let (state, places) = app.state_and_places("bank", false);

    assert_eq!(state.entries.len(), 1);
    assert_eq!(places.len(), 2);
    assert_eq!(places.get(&state.entries[0].id), Some(&1));
}

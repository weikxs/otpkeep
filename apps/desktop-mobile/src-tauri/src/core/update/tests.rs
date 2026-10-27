use std::sync::Arc;
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::store::VaultPaths;

use super::EntryUpdate;
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-update-")
        .tempdir()
        .unwrap()
}

/// A vault with one entry, and its id.
fn with_one(dir: &std::path::Path) -> (Core, String) {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );
    let view = app
        .stage_text("otpauth://totp/Example%20Cloud:alex@example.com?secret=JBSWY3DPEHPK3PXP")
        .unwrap();
    let one = Choice {
        fresh: vec![true],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &one).unwrap();
    let id = app.list("").unwrap().remove(0).id;
    (app, id)
}

fn everything() -> EntryUpdate {
    EntryUpdate {
        title: Some("Example Mail".to_owned()),
        note: Some("demo note".to_owned()),
        issuer: Some("Example Mail".to_owned()),
        account: Some("demo@example.com".to_owned()),
        tags: Some(vec![
            " work ".to_owned(),
            "Work".to_owned(),
            "演示".to_owned(),
        ]),
        pinned: Some(true),
    }
}

#[test]
fn one_save_changes_every_field_and_writes_the_vault_once() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = app.revision().unwrap();

    app.update(&id, everything()).unwrap();

    assert_eq!(app.revision(), Some(before + 1));
    let row = app.state_all("").entries.remove(0);
    assert_eq!(row.title, "Example Mail");
    assert_eq!(row.note, "demo note");
    assert_eq!(row.issuer, "Example Mail");
    assert_eq!(row.account, "demo@example.com");
    assert_eq!(row.tags, ["work", "演示"]);
    assert!(row.pinned);
    assert_eq!(app.history(&id).unwrap().title[0].value, "Example Cloud");
}

#[test]
fn one_bad_field_saves_none_of_them() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = app.revision().unwrap();
    let update = EntryUpdate {
        tags: Some(vec!["t".repeat(31)]),
        ..everything()
    };

    let result = app.update(&id, update);

    assert_eq!(result, Err(CoreError::Invalid("TAG_TOO_LONG")));
    assert_eq!(app.revision(), Some(before));
    assert_eq!(app.list("").unwrap()[0].title, "Example Cloud");
}

#[test]
fn an_empty_title_is_refused_before_anything_is_saved() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let update = EntryUpdate {
        title: Some("  ".to_owned()),
        ..EntryUpdate::default()
    };

    assert_eq!(
        app.update(&id, update),
        Err(CoreError::Invalid("TITLE_EMPTY"))
    );
}

#[test]
fn saving_without_changes_writes_nothing() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = app.revision();

    app.update(&id, EntryUpdate::default()).unwrap();

    assert_eq!(app.revision(), before);
}

#[test]
fn an_unknown_entry_is_reported() {
    let dir = temp_dir();
    let (mut app, _) = with_one(dir.path());

    assert_eq!(
        app.update("not-an-id", everything()),
        Err(CoreError::NotFound)
    );
    assert_eq!(
        app.update("00000000-0000-4000-8000-000000000000", everything()),
        Err(CoreError::NotFound)
    );
}

#[test]
fn the_editor_sends_only_what_changed_as_camel_case_fields() {
    let update: EntryUpdate =
        serde_json::from_str(r#"{"title":"Demo Git","pinned":false}"#).unwrap();

    assert_eq!(
        update,
        EntryUpdate {
            title: Some("Demo Git".to_owned()),
            pinned: Some(false),
            ..EntryUpdate::default()
        }
    );
}

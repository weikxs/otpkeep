use std::sync::Arc;
use std::time::Duration;

use keep_platform::{Keystore, KeystoreError, MemoryKeystore};
use keep_vault::edit::TRASH_RETENTION_MS;
use keep_vault::store::VaultPaths;
use zeroize::Zeroizing;

use super::MoveTo;
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

/// One in-memory keystore shared by two starts of the app, as the system keychain would be.
#[derive(Clone, Default)]
struct SharedKeystore(Arc<MemoryKeystore>);

impl Keystore for SharedKeystore {
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeystoreError> {
        self.0.load(account)
    }

    fn store(&self, account: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        self.0.store(account, secret)
    }

    fn delete(&self, account: &str) -> Result<(), KeystoreError> {
        self.0.delete(account)
    }
}

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-manage-")
        .tempdir()
        .unwrap()
}

/// A vault with A, B and C (in that order), added through the import flow.
fn with_three(dir: &std::path::Path, keystore: SharedKeystore, now: u64) -> Core {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(keystore),
        Arc::new(move || now),
        WAIT,
    );
    let view = app
        .stage_text(
            "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP\n\
             otpauth://totp/B:b?secret=GEZDGNBVGY3TQOJQ\n\
             otpauth://totp/C:c?secret=GEZDGNBVGY3TQOJQGEZDGNBV",
        )
        .unwrap();
    let all = keep_transfer::import::Choice {
        fresh: vec![true; 3],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &all).unwrap();
    app
}

fn titles(app: &Core) -> Vec<String> {
    app.list("").unwrap().into_iter().map(|e| e.title).collect()
}

fn id_of(app: &Core, title: &str) -> String {
    app.list("")
        .unwrap()
        .into_iter()
        .find(|e| e.title == title)
        .unwrap()
        .id
}

#[test]
fn pinning_brings_an_entry_to_the_top() {
    let dir = temp_dir();
    let mut app = with_three(dir.path(), SharedKeystore::default(), NOW_MS);

    app.set_pinned(&id_of(&app, "C"), true).unwrap();

    assert_eq!(titles(&app), ["C", "A", "B"]);
    assert!(app.state_all("").entries[0].pinned);
    app.set_pinned(&id_of(&app, "C"), false).unwrap();
    assert_eq!(titles(&app), ["A", "B", "C"]);
}

#[test]
fn moves_before_after_first_and_last() {
    let dir = temp_dir();
    let mut app = with_three(dir.path(), SharedKeystore::default(), NOW_MS);

    app.move_to(
        &id_of(&app, "C"),
        &MoveTo::Before {
            other: id_of(&app, "A"),
        },
    )
    .unwrap();
    assert_eq!(titles(&app), ["C", "A", "B"]);
    app.move_to(
        &id_of(&app, "C"),
        &MoveTo::After {
            other: id_of(&app, "B"),
        },
    )
    .unwrap();
    assert_eq!(titles(&app), ["A", "B", "C"]);
    app.move_to(&id_of(&app, "B"), &MoveTo::First).unwrap();
    assert_eq!(titles(&app), ["B", "A", "C"]);
    app.move_to(&id_of(&app, "B"), &MoveTo::Last).unwrap();
    assert_eq!(titles(&app), ["A", "C", "B"]);
    assert_eq!(
        app.move_to("nope", &MoveTo::First),
        Err(CoreError::NotFound)
    );
}

#[test]
fn tags_are_cleaned_and_limited() {
    let dir = temp_dir();
    let mut app = with_three(dir.path(), SharedKeystore::default(), NOW_MS);
    let a = id_of(&app, "A");

    app.set_tags(
        &a,
        vec![
            " 工作 ".to_owned(),
            String::new(),
            "工作".to_owned(),
            "Bank".to_owned(),
        ],
    )
    .unwrap();

    assert_eq!(app.state_all("").entries[0].tags, ["工作", "Bank"]);
    assert_eq!(app.list("工作").unwrap().len(), 1, "search finds tags");
    assert_eq!(
        app.set_tags(&a, vec!["x".repeat(31)]),
        Err(CoreError::Invalid("TAG_TOO_LONG"))
    );
    assert_eq!(
        app.set_tags(&a, (0..21).map(|i| format!("t{i}")).collect()),
        Err(CoreError::Invalid("TOO_MANY_TAGS"))
    );
}

#[test]
fn the_trash_keeps_entries_until_restored_or_deleted() {
    let dir = temp_dir();
    let mut app = with_three(dir.path(), SharedKeystore::default(), NOW_MS);
    let b = id_of(&app, "B");

    app.trash(&b).unwrap();
    assert_eq!(titles(&app), ["A", "C"]);
    let trash = app.trash_list().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].title, "B");
    assert_eq!(
        trash[0].purge_at_ms,
        trash[0].trashed_at_ms + TRASH_RETENTION_MS
    );

    app.restore(&b).unwrap();
    assert_eq!(titles(&app), ["A", "B", "C"]);

    app.trash(&b).unwrap();
    app.purge(&b).unwrap();
    assert!(app.trash_list().unwrap().is_empty());
    assert_eq!(app.restore(&b), Err(CoreError::NotFound));
}

#[test]
fn old_trash_is_emptied_when_the_data_opens() {
    let dir = temp_dir();
    let keystore = SharedKeystore::default();
    let mut app = with_three(dir.path(), keystore.clone(), NOW_MS);
    app.trash(&id_of(&app, "A")).unwrap();
    drop(app);

    let later = NOW_MS + TRASH_RETENTION_MS + 1000;
    let reopened = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(keystore),
        Arc::new(move || later),
        WAIT,
    );

    assert!(reopened.trash_list().unwrap().is_empty());
    assert_eq!(titles(&reopened), ["B", "C"]);
}

#[test]
fn keeps_earlier_names_and_notes() {
    let dir = temp_dir();
    let mut app = with_three(dir.path(), SharedKeystore::default(), NOW_MS);
    let a = id_of(&app, "A");

    app.edit(&a, Some("A 工作".to_owned()), Some("第一版备注".to_owned()))
        .unwrap();
    app.edit(&a, None, Some("第二版备注".to_owned())).unwrap();

    let history = app.history(&a).unwrap();
    assert_eq!(history.title[0].value, "A");
    assert!(
        history
            .note
            .iter()
            .any(|v| v.value.is_empty() || v.value == "第一版备注")
    );
}

#[test]
fn nothing_changes_before_the_master_password() {
    let dir = temp_dir();
    let params = keep_vault::sealed::KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    drop(
        keep_vault::open::create_with_master_password(
            &VaultPaths::new(dir.path()),
            "pw",
            params,
            WAIT,
        )
        .unwrap(),
    );
    let mut app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );

    assert_eq!(app.set_pinned("x", true), Err(CoreError::NeedsPassword));
    assert_eq!(app.trash_list(), Err(CoreError::NeedsPassword));
}

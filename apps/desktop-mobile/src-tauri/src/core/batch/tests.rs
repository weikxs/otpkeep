use std::sync::Arc;
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::store::VaultPaths;

use super::BatchAction;
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-batch-")
        .tempdir()
        .unwrap()
}

fn with_three(dir: &std::path::Path) -> (Core, Vec<String>) {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );
    let view = app
        .stage_text(
            "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP\n\
             otpauth://totp/B:b?secret=GEZDGNBVGY3TQOJQ\n\
             otpauth://totp/C:c?secret=GEZDGNBVGY3TQOJQGEZDGNBV",
        )
        .unwrap();
    let all = Choice {
        fresh: vec![true; 3],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &all).unwrap();
    let ids = app.list("").unwrap().into_iter().map(|e| e.id).collect();
    (app, ids)
}

fn tags_of(app: &Core) -> Vec<Vec<String>> {
    app.state_all("")
        .entries
        .into_iter()
        .map(|e| e.tags)
        .collect()
}

#[test]
fn adds_a_tag_to_the_chosen_entries_once() {
    let dir = temp_dir();
    let (mut app, ids) = with_three(dir.path());

    assert_eq!(
        app.batch(
            &ids[..2],
            &BatchAction::AddTag {
                tag: " 工作 ".to_owned()
            }
        ),
        Ok(2)
    );
    assert_eq!(
        app.batch(
            &ids,
            &BatchAction::AddTag {
                tag: "工作".to_owned()
            }
        ),
        Ok(1)
    );

    assert_eq!(tags_of(&app), [vec!["工作"], vec!["工作"], vec!["工作"]]);
}

#[test]
fn removes_a_tag_where_it_is() {
    let dir = temp_dir();
    let (mut app, ids) = with_three(dir.path());
    app.batch(
        &ids,
        &BatchAction::AddTag {
            tag: "旧".to_owned(),
        },
    )
    .unwrap();

    assert_eq!(
        app.batch(
            &ids[1..],
            &BatchAction::RemoveTag {
                tag: "旧".to_owned()
            }
        ),
        Ok(2)
    );

    assert_eq!(tags_of(&app), [vec!["旧".to_owned()], vec![], vec![]]);
}

#[test]
fn moves_the_chosen_entries_to_the_trash_together() {
    let dir = temp_dir();
    let (mut app, ids) = with_three(dir.path());

    assert_eq!(app.batch(&ids[..2], &BatchAction::Trash), Ok(2));

    assert_eq!(app.list("").unwrap().len(), 1);
    assert_eq!(app.trash_list().unwrap().len(), 2);
}

#[test]
fn changes_nothing_when_one_entry_fails() {
    let dir = temp_dir();
    let (mut app, ids) = with_three(dir.path());
    let mut chosen = ids.clone();
    chosen.push("missing".to_owned());

    assert_eq!(
        app.batch(&chosen, &BatchAction::Trash),
        Err(CoreError::NotFound)
    );
    assert_eq!(
        app.batch(
            &ids,
            &BatchAction::AddTag {
                tag: "x".repeat(31)
            }
        ),
        Err(CoreError::Invalid("TAG_TOO_LONG"))
    );
    assert_eq!(
        app.batch(
            &ids,
            &BatchAction::AddTag {
                tag: " ".to_owned()
            }
        ),
        Err(CoreError::Invalid("TAG_EMPTY"))
    );

    assert_eq!(app.list("").unwrap().len(), 3);
    assert_eq!(tags_of(&app), [Vec::<String>::new(), vec![], vec![]]);
}

/// A stamp without its device part, which is random per vault.
type Stamp = (u64, u32);

/// One entry's title, tags, trash state and the stamps of both.
type Changes = (String, Vec<String>, Stamp, bool, Stamp);

/// Every entry's [`Changes`], by title (ids are random per vault).
fn changes_of(app: &Core) -> Vec<Changes> {
    let crate::core::Vault::Open { service, .. } = &app.vault else {
        panic!("the vault is not open");
    };
    let stamp = |s: keep_vault::Hlc| (s.wall_ms, s.logical);
    let mut entries: Vec<_> = service.doc().entries.iter().collect();
    entries.sort_by(|a, b| a.title.value.cmp(&b.title.value));
    entries
        .into_iter()
        .map(|e| {
            (
                e.title.value.clone(),
                e.tags.value.clone(),
                stamp(e.tags.stamp),
                e.trashed_at.value.is_some(),
                stamp(e.trashed_at.stamp),
            )
        })
        .collect()
}

#[test]
fn a_batch_saves_once_what_one_change_after_another_would_save() {
    let (one_dir, other_dir) = (temp_dir(), temp_dir());
    let (mut batched, ids) = with_three(one_dir.path());
    let (mut stepped, step_ids) = with_three(other_dir.path());
    let before = batched.revision().unwrap();

    batched
        .batch(
            &ids,
            &BatchAction::AddTag {
                tag: "work".to_owned(),
            },
        )
        .unwrap();
    batched.batch(&ids[1..], &BatchAction::Trash).unwrap();
    for id in &step_ids {
        stepped.set_tags(id, vec!["work".to_owned()]).unwrap();
    }
    for id in &step_ids[1..] {
        stepped.trash(id).unwrap();
    }

    assert_eq!(batched.revision(), Some(before + 2));
    assert_eq!(changes_of(&batched), changes_of(&stepped));
}

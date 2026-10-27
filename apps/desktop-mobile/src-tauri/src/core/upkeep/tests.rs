use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::edit::TRASH_RETENTION_MS;
use keep_vault::store::VaultPaths;

use crate::core::Core;
use crate::staging::STAGED_TTL_MS;

const WAIT: Duration = Duration::from_millis(100);
const START_MS: u64 = 1_700_000_020_000;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const LINK: &str = "otpauth://totp/Example%20Cloud:demo-user?secret=JBSWY3DPEHPK3PXP";

struct Fixture {
    _dir: tempfile::TempDir,
    now: Arc<AtomicU64>,
    core: Core,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("app-upkeep-")
            .tempdir()
            .unwrap();
        let now = Arc::new(AtomicU64::new(START_MS));
        let clock = Arc::clone(&now);
        let core = Core::start(
            Some(VaultPaths::new(dir.path())),
            Box::new(MemoryKeystore::new()),
            Arc::new(move || clock.load(Ordering::SeqCst)),
            WAIT,
        );
        Self {
            _dir: dir,
            now,
            core,
        }
    }

    fn pass(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

#[test]
fn staged_imports_with_their_secrets_go_once_too_old() {
    let mut fixture = Fixture::new();
    fixture.core.stage_text(LINK).unwrap();
    fixture.core.upkeep();
    assert_eq!(fixture.core.staged_count(), 1);

    fixture.pass(STAGED_TTL_MS + 1);
    fixture.core.upkeep();

    assert_eq!(fixture.core.staged_count(), 0);
}

#[test]
fn closing_the_main_window_drops_staged_imports() {
    let mut fixture = Fixture::new();
    fixture.core.stage_text(LINK).unwrap();

    fixture.core.cancel_all_staged();

    assert_eq!(fixture.core.staged_count(), 0);
}

#[test]
fn old_trash_is_emptied_while_the_app_stays_open() {
    let mut fixture = Fixture::new();
    let view = fixture.core.stage_text(LINK).unwrap();
    let all = Choice {
        fresh: vec![true],
        duplicates: Vec::new(),
    };
    fixture.core.commit_staged(&view.token, &all).unwrap();
    let id = fixture.core.list("").unwrap()[0].id.clone();
    fixture.core.trash(&id).unwrap();

    fixture.pass(TRASH_RETENTION_MS - DAY_MS);
    fixture.core.upkeep();
    assert_eq!(
        fixture.core.trash_list().unwrap().len(),
        1,
        "not old enough yet"
    );

    fixture.pass(2 * DAY_MS);
    fixture.core.upkeep();
    assert!(fixture.core.trash_list().unwrap().is_empty());
}

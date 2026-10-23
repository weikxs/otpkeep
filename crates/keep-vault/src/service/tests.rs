use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use keep_passcode::{
    CodeKind, CodeSpec, Digest, Moment, PasscodeError, SecretKey, derive_passcode,
};
use keep_platform::MemoryKeystore;

use super::{ServiceError, VaultService};
use crate::edit::{Change, EditError, NewEntry};
use crate::history::HistoryField;
use crate::open::{SaveError, Startup, create_vault, start};
use crate::store::{VaultPaths, test_dir};

const WAIT: Duration = Duration::from_millis(200);
const MINUTE_MS: u64 = 60_000;

/// A clock the test moves by hand.
#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn at(ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(ms)))
    }
    fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
    fn source(&self) -> impl Fn() -> u64 + Send + Sync + 'static {
        let time = Arc::clone(&self.0);
        move || time.load(Ordering::SeqCst)
    }
}

fn new_entry(title: &str, byte: u8, spec: CodeSpec) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "alex@example.com".to_owned(),
        secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
        spec,
        counter: 0,
        note: String::new(),
        tags: vec![],
        keep_duplicate: false,
    }
}

fn hotp() -> CodeSpec {
    CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap()
}

/// A new vault in a fresh directory, and a service on it.
fn service(clock: &TestClock) -> (tempfile::TempDir, MemoryKeystore, VaultService) {
    let dir = test_dir();
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir.path()), &keystore, WAIT).unwrap();
    let service = VaultService::new(open, clock.source());
    (dir, keystore, service)
}

/// Closes and reopens the vault, as a restart would.
fn reopen(
    dir: &tempfile::TempDir,
    keystore: &MemoryKeystore,
    service: VaultService,
    clock: &TestClock,
) -> VaultService {
    drop(service);
    match start(&VaultPaths::new(dir.path()), keystore, WAIT) {
        Startup::Ready(open) => VaultService::new(open, clock.source()),
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn changes_are_saved_before_they_count() {
    let clock = TestClock::at(1_000);
    let (dir, keystore, mut service) = service(&clock);

    let id = service
        .add(new_entry("Cloud", 1, CodeSpec::default()))
        .unwrap();
    service.edit(id, Change::Note("备注".into())).unwrap();
    let service = reopen(&dir, &keystore, service, &clock);

    let entry = service.entry(id).unwrap();
    assert_eq!(entry.title.value, "Cloud");
    assert_eq!(entry.note.value, "备注");
}

#[test]
fn a_rejected_edit_changes_nothing() {
    let clock = TestClock::at(1_000);
    let (dir, _keystore, mut service) = service(&clock);
    let id = service
        .add(new_entry("Cloud", 1, CodeSpec::default()))
        .unwrap();
    let before = fs::read(VaultPaths::new(dir.path()).vault()).unwrap();
    let doc_before = service.doc().clone();

    let result = service.edit(id, Change::Title("  ".into()));

    assert_eq!(
        result.err(),
        Some(ServiceError::Edit(EditError::TitleEmpty))
    );
    assert_eq!(service.doc(), &doc_before);
    assert_eq!(
        fs::read(VaultPaths::new(dir.path()).vault()).unwrap(),
        before
    );
}

#[test]
fn a_failed_save_changes_nothing() {
    let clock = TestClock::at(1_000);
    let (dir, _keystore, mut service) = service(&clock);
    let doc_before = service.doc().clone();
    fs::create_dir(VaultPaths::new(dir.path()).tmp()).unwrap();

    let result = service.add(new_entry("Cloud", 1, CodeSpec::default()));

    assert_eq!(result.err(), Some(ServiceError::Save(SaveError::Io)));
    assert_eq!(service.doc(), &doc_before);
}

#[test]
fn renaming_and_editing_notes_keep_history_across_restarts() {
    let clock = TestClock::at(0);
    let (dir, keystore, mut service) = service(&clock);
    let id = service
        .add(new_entry("first name", 1, CodeSpec::default()))
        .unwrap();
    service.edit(id, Change::Note("first note".into())).unwrap();

    clock.set(60 * MINUTE_MS);
    service
        .edit(id, Change::Title("second name".into()))
        .unwrap();
    service
        .edit(id, Change::Note("second note".into()))
        .unwrap();
    let service = reopen(&dir, &keystore, service, &clock);

    let titles: Vec<_> = service
        .history(id, HistoryField::Title)
        .into_iter()
        .map(|v| v.value.clone())
        .collect();
    let notes: Vec<_> = service
        .history(id, HistoryField::Note)
        .into_iter()
        .map(|v| v.value.clone())
        .collect();
    assert_eq!(titles, ["first name"]);
    assert_eq!(notes, ["first note", ""]);
}

#[test]
fn purging_an_entry_forgets_its_history() {
    let clock = TestClock::at(0);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service.add(new_entry("a", 1, CodeSpec::default())).unwrap();
    service.edit(id, Change::Title("b".into())).unwrap();

    service.trash(id).unwrap();
    service.purge(id).unwrap();

    assert!(service.history(id, HistoryField::Title).is_empty());
    assert!(service.entry(id).is_none());
}

#[test]
fn totp_codes_follow_the_clock() {
    let clock = TestClock::at(59_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service.add(new_entry("t", 1, CodeSpec::default())).unwrap();
    let secret = SecretKey::from_bytes(vec![1; 20]).unwrap();

    let code = service.passcode(id).unwrap();

    let expected = derive_passcode(&secret, CodeSpec::default(), Moment::Time(59)).unwrap();
    assert_eq!(code.code(), expected.code());
    assert_eq!(code.valid_for_s(), Some(1));
}

#[test]
fn hotp_codes_advance_only_when_the_next_code_is_asked_for() {
    let clock = TestClock::at(1_000);
    let (dir, keystore, mut service) = service(&clock);
    let id = service
        .add(NewEntry {
            counter: 7,
            ..new_entry("h", 1, hotp())
        })
        .unwrap();
    let secret = SecretKey::from_bytes(vec![1; 20]).unwrap();

    assert_eq!(service.passcode(id).err(), Some(ServiceError::NoCodeYet));
    let generated = service.next_hotp(id).unwrap();
    let copied_twice = (service.passcode(id).unwrap(), service.passcode(id).unwrap());

    let expected = derive_passcode(&secret, hotp(), Moment::Counter(7)).unwrap();
    assert_eq!(generated.code(), expected.code());
    assert_eq!(copied_twice.0.code(), expected.code());
    assert_eq!(copied_twice.1.code(), expected.code());
    assert_eq!(service.entry(id).unwrap().key.value.counter, 8);
    let service = reopen(&dir, &keystore, service, &clock);
    assert_eq!(service.entry(id).unwrap().key.value.counter, 8);
    assert_eq!(
        service.passcode(id).err(),
        Some(ServiceError::NoCodeYet),
        "codes are not kept on disk"
    );
}

#[test]
fn md5_entries_say_their_algorithm_is_unsupported() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let md5 = CodeSpec::new(CodeKind::TimeBased { period_s: 30 }, Digest::Md5, 6).unwrap();
    let id = service.add(new_entry("m", 1, md5)).unwrap();

    assert_eq!(
        service.passcode(id).err(),
        Some(ServiceError::Passcode(PasscodeError::UnsupportedDigest(
            Digest::Md5
        )))
    );
}

#[test]
fn old_trash_is_emptied_on_request() {
    let clock = TestClock::at(0);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service.add(new_entry("a", 1, CodeSpec::default())).unwrap();
    service.trash(id).unwrap();

    clock.set(31 * 24 * 60 * MINUTE_MS);
    service.purge_expired_trash().unwrap();

    assert!(service.entry(id).is_none());
}

#[test]
fn a_damaged_history_file_does_not_block_the_vault() {
    let clock = TestClock::at(0);
    let (dir, keystore, mut service) = service(&clock);
    let id = service.add(new_entry("a", 1, CodeSpec::default())).unwrap();
    service.edit(id, Change::Title("b".into())).unwrap();
    let paths = VaultPaths::new(dir.path());
    drop(service);
    fs::write(paths.local(), b"damaged").unwrap();

    let service = reopen_after_drop(&dir, &keystore, &clock);

    assert_eq!(service.entry(id).unwrap().title.value, "b");
    assert!(service.history(id, HistoryField::Title).is_empty());
    assert!(
        fs::read_dir(dir.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("local.okv.damaged")),
        "the damaged history file is kept aside"
    );
}

fn reopen_after_drop(
    dir: &tempfile::TempDir,
    keystore: &MemoryKeystore,
    clock: &TestClock,
) -> VaultService {
    match start(&VaultPaths::new(dir.path()), keystore, WAIT) {
        Startup::Ready(open) => VaultService::new(open, clock.source()),
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn moving_and_pinning_go_through_the_service() {
    let clock = TestClock::at(0);
    let (_dir, _keystore, mut service) = service(&clock);
    let a = service.add(new_entry("a", 1, CodeSpec::default())).unwrap();
    let b = service.add(new_entry("b", 2, CodeSpec::default())).unwrap();

    service
        .move_entry(b, crate::edit::Placement::First)
        .unwrap();
    let order: Vec<_> = service.ordered().iter().map(|e| e.id).collect();

    assert_eq!(order, [b, a]);
}

#[test]
fn restoring_from_the_trash_and_replacing_a_key() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service
        .add(NewEntry {
            counter: 1,
            ..new_entry("h", 1, hotp())
        })
        .unwrap();
    service.next_hotp(id).unwrap();

    service.trash(id).unwrap();
    service.restore(id).unwrap();
    service
        .replace_key(id, SecretKey::from_bytes(vec![2; 20]).unwrap(), hotp(), 0)
        .unwrap();

    assert!(service.ordered().iter().any(|e| e.id == id));
    assert_eq!(
        service.trashed().len(),
        1,
        "the old key is kept in the trash"
    );
    assert_eq!(
        service.passcode(id).err(),
        Some(ServiceError::NoCodeYet),
        "a new key has no code yet"
    );
}

#[test]
fn unknown_entries_are_reported() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let missing = crate::EntryId([9; 16]);

    assert_eq!(
        service.passcode(missing).err(),
        Some(ServiceError::NotFound)
    );
    assert_eq!(
        service.next_hotp(missing).err(),
        Some(ServiceError::NotFound)
    );
    assert_eq!(
        service.edit(missing, Change::Pinned(true)).err(),
        Some(ServiceError::NotFound)
    );
    assert_eq!(
        service.trash(missing).err(),
        Some(ServiceError::Edit(EditError::NotFound))
    );
}

#[test]
fn md5_hotp_entries_do_not_step_their_counter() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let md5 = CodeSpec::new(CodeKind::CounterBased, Digest::Md5, 6).unwrap();
    let id = service
        .add(NewEntry {
            counter: 3,
            ..new_entry("m", 1, md5)
        })
        .unwrap();

    assert!(matches!(
        service.next_hotp(id),
        Err(ServiceError::Passcode(_))
    ));
    assert!(matches!(
        service.passcode(id),
        Err(ServiceError::Passcode(_))
    ));
    assert_eq!(service.entry(id).unwrap().key.value.counter, 3);
}

#[test]
fn debug_output_shows_no_entry_contents() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    service
        .add(new_entry("Secret Bank", 1, CodeSpec::default()))
        .unwrap();

    let shown = format!("{service:?}");

    assert!(!shown.contains("Secret Bank"), "{shown}");
}

#[test]
fn nothing_expired_means_nothing_is_written() {
    let clock = TestClock::at(0);
    let (dir, _keystore, mut service) = service(&clock);
    service.add(new_entry("a", 1, CodeSpec::default())).unwrap();
    let before = fs::read(VaultPaths::new(dir.path()).vault()).unwrap();

    service.purge_expired_trash().unwrap();

    assert_eq!(
        fs::read(VaultPaths::new(dir.path()).vault()).unwrap(),
        before
    );
}

#[test]
fn accepts_a_document_derived_from_the_current_one() {
    let clock = TestClock::at(1_000);
    let (dir, keystore, mut service) = service(&clock);
    let (derived, id) = crate::edit::add_entry(
        service.doc(),
        new_entry("from an import", 9, CodeSpec::default()),
        2_000,
    )
    .unwrap();

    service.commit_derived(derived).unwrap();
    let service = reopen(&dir, &keystore, service, &clock);

    assert_eq!(service.entry(id).unwrap().title.value, "from an import");
}

#[test]
fn refuses_documents_not_derived_from_the_current_one() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let (stale, _) = crate::edit::add_entry(
        service.doc(),
        new_entry("stale", 8, CodeSpec::default()),
        2_000,
    )
    .unwrap();
    service
        .add(new_entry("newer", 7, CodeSpec::default()))
        .unwrap();
    let other_vault = crate::VaultDoc {
        vault_id: [0xEE; 16],
        ..service.doc().clone()
    };

    assert_eq!(
        service.commit_derived(stale).err(),
        Some(ServiceError::NotDerived)
    );
    assert_eq!(
        service.commit_derived(other_vault).err(),
        Some(ServiceError::NotDerived)
    );
}

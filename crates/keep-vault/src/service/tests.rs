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

#[test]
fn a_synced_state_replaces_the_vault_and_keeps_what_it_overwrote_in_the_history() {
    let clock = TestClock::at(MINUTE_MS);
    let (dir, keystore, mut service) = service(&clock);
    let kept = service
        .add(new_entry("Old name", 1, CodeSpec::default()))
        .unwrap();
    let dropped = service
        .add(new_entry("Deleted elsewhere", 2, CodeSpec::default()))
        .unwrap();
    let mut synced = service.doc().clone();
    synced.entries.retain(|e| e.id != dropped);
    synced.entries[0].title.value = "New name".to_owned();
    synced.clock.wall_ms += 1;

    service
        .commit_synced(
            synced,
            &[(kept, HistoryField::Title, "Old name".to_owned())],
        )
        .unwrap();
    let service = reopen(&dir, &keystore, service, &clock);

    assert!(service.entry(dropped).is_none());
    assert_eq!(service.entry(kept).unwrap().title.value, "New name");
    let history = service.history(kept, HistoryField::Title);
    assert_eq!(history.first().map(|v| v.value.as_str()), Some("Old name"));
}

#[test]
fn a_synced_state_from_another_vault_or_the_past_is_refused() {
    let clock = TestClock::at(MINUTE_MS);
    let (_dir, _keystore, mut service) = service(&clock);
    service.add(new_entry("A", 1, CodeSpec::default())).unwrap();
    let mut other = service.doc().clone();
    other.vault_id = [9; 16];
    let mut older = service.doc().clone();
    older.clock.wall_ms -= 1;

    assert!(matches!(
        service.commit_synced(other, &[]),
        Err(ServiceError::NotDerived)
    ));
    assert!(matches!(
        service.commit_synced(older, &[]),
        Err(ServiceError::NotDerived)
    ));
}

#[test]
fn a_backup_before_pairing_is_written_encrypted_in_the_backups_folder() {
    let clock = TestClock::at(MINUTE_MS);
    let (dir, _keystore, mut service) = service(&clock);
    service
        .add(new_entry("Before pairing", 1, CodeSpec::default()))
        .unwrap();

    let path = service.backup_before_pairing().unwrap();

    assert!(path.starts_with(dir.path().join("backups")));
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"OKV\0"));
    assert!(
        !bytes.windows(14).any(|w| w == b"Before pairing"),
        "the backup is encrypted"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn several_changes_to_one_entry_are_saved_once_with_their_history() {
    let clock = TestClock::at(0);
    let (dir, keystore, mut service) = service(&clock);
    let id = service
        .add(new_entry("first name", 1, CodeSpec::default()))
        .unwrap();
    let before = service.revision();

    clock.set(MINUTE_MS);
    service
        .edit_all(
            id,
            vec![
                Change::Title("second name".into()),
                Change::Note("a note".into()),
                Change::Issuer("Example Mail".into()),
                Change::Tags(vec!["work".into()]),
                Change::Pinned(true),
            ],
        )
        .unwrap();

    assert_eq!(service.revision(), before + 1);
    let service = reopen(&dir, &keystore, service, &clock);
    let entry = service.entry(id).unwrap();
    assert_eq!(entry.title.value, "second name");
    assert_eq!(entry.issuer.value, "Example Mail");
    assert!(entry.pinned.value);
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
    assert_eq!(notes, [""]);
}

#[test]
fn one_rejected_change_among_several_saves_nothing() {
    let clock = TestClock::at(0);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service
        .add(new_entry("first name", 1, CodeSpec::default()))
        .unwrap();
    let before = (service.revision(), service.doc().clone());

    let result = service.edit_all(
        id,
        vec![Change::Note("a note".into()), Change::Title(String::new())],
    );

    assert_eq!(result, Err(ServiceError::Edit(EditError::TitleEmpty)));
    assert_eq!((service.revision(), service.doc().clone()), before);
}

#[test]
fn entries_are_found_after_others_are_deleted() {
    let clock = TestClock::at(0);
    let (_dir, _keystore, mut service) = service(&clock);
    let first = service
        .add(new_entry("first", 1, CodeSpec::default()))
        .unwrap();
    let second = service
        .add(new_entry("second", 2, CodeSpec::default()))
        .unwrap();

    service.purge(first).unwrap();

    assert!(service.entry(first).is_none());
    assert_eq!(service.entry(second).unwrap().title.value, "second");
}

#[test]
fn refuses_a_document_with_an_older_field_than_the_current_one() {
    let clock = TestClock::at(1_000);
    let (_dir, _keystore, mut service) = service(&clock);
    let id = service
        .add(new_entry("first", 1, CodeSpec::default()))
        .unwrap();
    let older = service.doc().clone();
    service.edit(id, Change::Title("second".into())).unwrap();
    let mut rolled_back = service.doc().clone();
    rolled_back.entries = older.entries;

    assert_eq!(
        service.commit_derived(rolled_back).err(),
        Some(ServiceError::NotDerived)
    );
}

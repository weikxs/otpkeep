use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeSpec, SecretKey};
use keep_platform::{Keystore, KeystoreError, MemoryKeystore};
use keep_vault::edit::NewEntry;
use keep_vault::open::create_vault;
use keep_vault::service::VaultService;
use keep_vault::store::{VaultLock, VaultPaths};
use zeroize::Zeroizing;

use crate::core::{Core, CoreError, PanelStatus, Problem};

const WAIT: Duration = Duration::from_millis(100);
const NOW_MS: u64 = 1_700_000_020_000;

/// One keystore seen by the vault's creator and by the core, as the system one would be.
struct Shared(Arc<MemoryKeystore>);

impl Keystore for Shared {
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
        .prefix("app-reopen-")
        .tempdir_in(std::env::temp_dir())
        .unwrap()
}

fn start(dir: &Path, keystore: &Arc<MemoryKeystore>) -> Core {
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(Shared(Arc::clone(keystore))),
        Arc::new(|| NOW_MS),
        WAIT,
    )
}

fn demo(title: &str, byte: u8) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "demo@example.com".to_owned(),
        secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: Vec::new(),
        keep_duplicate: false,
    }
}

/// A vault with two saved versions whose latest file is then damaged.
fn damaged_vault(dir: &Path, keystore: &MemoryKeystore) {
    let paths = VaultPaths::new(dir);
    let open = create_vault(&paths, keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || NOW_MS);
    service.add(demo("Example Cloud", 1)).unwrap();
    service.add(demo("Example Mail", 2)).unwrap();
    drop(service);
    let mut bytes = fs::read(paths.vault()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(paths.vault(), &bytes).unwrap();
}

#[test]
fn a_busy_vault_opens_on_retry_once_released() {
    let dir = temp_dir();
    let keystore = Arc::new(MemoryKeystore::new());
    let held = VaultLock::acquire(&VaultPaths::new(dir.path()), WAIT).unwrap();
    let mut core = start(dir.path(), &keystore);
    assert_eq!(core.state("").problem, Some(Problem::Busy));

    drop(held);
    let state = core.retry_open();

    assert_eq!(state.status, PanelStatus::Ready);
    assert_eq!(state.problem, None);
}

#[test]
fn retrying_an_open_vault_changes_nothing() {
    let dir = temp_dir();
    let keystore = Arc::new(MemoryKeystore::new());
    let mut core = start(dir.path(), &keystore);

    assert_eq!(core.retry_open().status, PanelStatus::Ready);
}

#[test]
fn a_damaged_vault_offers_and_restores_its_previous_version() {
    let dir = temp_dir();
    let keystore = Arc::new(MemoryKeystore::new());
    damaged_vault(dir.path(), &keystore);
    let mut core = start(dir.path(), &keystore);
    let state = core.state("");
    assert_eq!(state.problem, Some(Problem::Damaged));
    assert!(state.can_restore);

    let state = core.restore_previous().unwrap();

    assert_eq!(state.status, PanelStatus::Ready);
    assert!(!state.can_restore);
    assert_eq!(state.entries.len(), 1, "the version before the last save");
}

#[test]
fn restoring_needs_a_damaged_vault() {
    let dir = temp_dir();
    let keystore = Arc::new(MemoryKeystore::new());
    let mut core = start(dir.path(), &keystore);

    assert_eq!(
        core.restore_previous(),
        Err(CoreError::Invalid("NOTHING_TO_RESTORE"))
    );
}

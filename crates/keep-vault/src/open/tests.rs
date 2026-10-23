use std::fs;
use std::time::Duration;

use keep_platform::{Keystore, MemoryKeystore};

use super::{
    KEYSTORE_ACCOUNT, OpenVault, SaveError, Startup, StopReason, UnlockError, create_vault,
    create_with_master_password, restore_previous, start, unlock_with_password,
};
use crate::sealed::{DataKey, FileKind, KdfParams, KeySource, SEALED_FORMAT, SealedHeader, seal};
use crate::store::{VaultLock, VaultPaths, test_dir};
use crate::{DeviceTag, Hlc, VaultDoc};

const WAIT: Duration = Duration::from_millis(200);

fn quick_kdf() -> KdfParams {
    KdfParams {
        salt: [4; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

fn ready(startup: Startup) -> OpenVault {
    match startup {
        Startup::Ready(open) => open,
        other => panic!("expected Ready, got {other:?}"),
    }
}

fn stopped(startup: Startup) -> StopReason {
    match startup {
        Startup::Stopped(reason) => reason,
        other => panic!("expected Stopped, got {other:?}"),
    }
}

fn with_clock(doc: &VaultDoc, wall_ms: u64) -> VaultDoc {
    VaultDoc {
        clock: Hlc {
            wall_ms,
            logical: 0,
            device: DeviceTag([1; 8]),
        },
        ..doc.clone()
    }
}

#[test]
fn nothing_anywhere_is_a_fresh_start() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());

    assert!(matches!(
        start(&paths, &MemoryKeystore::new(), WAIT),
        Startup::Fresh {
            keystore_available: true
        }
    ));
    assert!(matches!(
        start(&paths, &MemoryKeystore::unavailable(), WAIT),
        Startup::Fresh {
            keystore_available: false
        }
    ));
}

#[test]
fn a_created_vault_opens_again_with_its_contents() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();

    let mut created = create_vault(&paths, &keystore, WAIT).unwrap();
    let saved = with_clock(created.doc(), 42);
    created.save(saved.clone()).unwrap();
    drop(created);
    let reopened = ready(start(&paths, &keystore, WAIT));

    assert_eq!(reopened.doc(), &saved);
    assert!(keystore.load(KEYSTORE_ACCOUNT).unwrap().is_some());
}

#[test]
fn creating_over_existing_data_is_refused() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());

    assert!(create_vault(&paths, &keystore, WAIT).is_err());
    assert!(create_vault(&paths, &MemoryKeystore::new(), WAIT).is_err());
}

#[test]
fn a_file_without_its_key_stops() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    keystore.delete(KEYSTORE_ACCOUNT).unwrap();
    let before = fs::read(paths.vault()).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::KeyMissing
    );
    assert_eq!(
        fs::read(paths.vault()).unwrap(),
        before,
        "the file must not change"
    );
}

#[test]
fn a_key_for_a_different_vault_counts_as_missing() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    keystore.store(KEYSTORE_ACCOUNT, &[0; 48]).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::KeyMissing
    );
}

#[test]
fn a_key_without_its_file_stops_instead_of_starting_fresh() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    fs::remove_file(paths.vault()).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::DataMissing
    );
}

#[test]
fn a_damaged_file_stops_and_can_be_restored_from_the_previous_version() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    let mut open = create_vault(&paths, &keystore, WAIT).unwrap();
    let earlier = with_clock(open.doc(), 1);
    open.save(earlier.clone()).unwrap();
    open.save(with_clock(open.doc(), 2)).unwrap();
    drop(open);
    let mut bytes = fs::read(paths.vault()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(paths.vault(), &bytes).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::Damaged {
            previous_available: true
        }
    );
    restore_previous(&paths).unwrap();
    assert_eq!(ready(start(&paths, &keystore, WAIT)).doc(), &earlier);
    assert!(
        fs::read_dir(dir.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("vault.okv.damaged")),
        "the damaged file is kept for diagnosis"
    );
}

#[test]
fn a_file_that_is_not_a_vault_is_damaged() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    fs::write(paths.vault(), b"not a vault").unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::Damaged {
            previous_available: false
        }
    );
}

#[test]
fn a_newer_file_format_stops_without_being_read() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    let newer = SealedHeader {
        format: SEALED_FORMAT + 1,
        ..SealedHeader::new(FileKind::Vault, KeySource::Keystore, [0; 16])
    };
    fs::write(
        paths.vault(),
        seal(&newer, &DataKey::from_bytes([0; 32]), b"x").unwrap(),
    )
    .unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::NewerFormat(SEALED_FORMAT + 1)
    );
}

#[test]
fn a_newer_versions_file_beside_ours_makes_the_vault_read_only() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    fs::write(dir.path().join("vault-v2.okv"), b"newer").unwrap();

    let Startup::ReadOnly {
        mut open,
        newer_version,
    } = start(&paths, &keystore, WAIT)
    else {
        panic!("expected read-only");
    };

    assert_eq!(newer_version, 2);
    assert_eq!(
        open.save(with_clock(open.doc(), 9)).err(),
        Some(SaveError::ReadOnly)
    );
}

#[test]
fn a_master_password_vault_asks_for_the_password() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let mut created =
        create_with_master_password(&paths, "主口令 demo", quick_kdf(), WAIT).unwrap();
    let saved = with_clock(created.doc(), 7);
    created.save(saved.clone()).unwrap();
    drop(created);

    let Startup::NeedsMasterPassword(params) = start(&paths, &MemoryKeystore::unavailable(), WAIT)
    else {
        panic!("expected a password prompt");
    };
    assert_eq!(params, quick_kdf());
    assert_eq!(
        unlock_with_password(&paths, "wrong", WAIT).err(),
        Some(UnlockError::WrongPassword)
    );
    assert_eq!(
        unlock_with_password(&paths, "主口令 demo", WAIT)
            .unwrap()
            .doc(),
        &saved
    );
}

#[test]
fn a_held_lock_stops_with_busy() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    let _other = VaultLock::acquire(&paths, WAIT).unwrap();

    assert_eq!(stopped(start(&paths, &keystore, WAIT)), StopReason::Busy);
}

#[test]
fn an_open_vault_holds_the_lock() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    let open = create_vault(&paths, &keystore, WAIT).unwrap();

    assert!(VaultLock::acquire(&paths, WAIT).is_err());
    drop(open);
    assert!(VaultLock::acquire(&paths, WAIT).is_ok());
}

#[test]
fn a_keystore_file_with_no_keystore_stops() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    drop(create_vault(&paths, &MemoryKeystore::new(), WAIT).unwrap());

    assert_eq!(
        stopped(start(&paths, &MemoryKeystore::unavailable(), WAIT)),
        StopReason::KeystoreUnavailable
    );
}

#[test]
fn a_failed_save_keeps_the_previous_contents_in_memory() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let mut open = create_vault(&paths, &MemoryKeystore::new(), WAIT).unwrap();
    let before = open.doc().clone();
    fs::create_dir(paths.tmp()).unwrap();

    assert_eq!(open.save(with_clock(&before, 5)).err(), Some(SaveError::Io));
    assert_eq!(open.doc(), &before);
}

#[test]
fn a_missing_vault_with_a_previous_copy_offers_to_restore_it() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    let mut open = create_vault(&paths, &keystore, WAIT).unwrap();
    open.save(with_clock(open.doc(), 3)).unwrap();
    drop(open);
    fs::remove_file(paths.vault()).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::Damaged {
            previous_available: true
        }
    );
    restore_previous(&paths).unwrap();
    assert!(matches!(start(&paths, &keystore, WAIT), Startup::Ready(_)));
}

#[test]
fn creating_while_the_keystore_already_has_a_key_is_refused() {
    let dir = test_dir();
    let keystore = MemoryKeystore::new();
    keystore.store(KEYSTORE_ACCOUNT, &[1; 48]).unwrap();

    assert_eq!(
        create_vault(&VaultPaths::new(dir.path()), &keystore, WAIT).err(),
        Some(super::CreateError::AlreadyExists)
    );
}

#[test]
fn a_failed_first_write_leaves_no_key_behind() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    fs::create_dir_all(paths.tmp()).unwrap();

    assert_eq!(
        create_vault(&paths, &keystore, WAIT).err(),
        Some(super::CreateError::Io)
    );
    assert_eq!(keystore.load(KEYSTORE_ACCOUNT).unwrap(), None);
}

#[test]
fn an_empty_password_and_a_busy_lock_are_reported_when_unlocking() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    drop(create_with_master_password(&paths, "demo", quick_kdf(), WAIT).unwrap());

    assert_eq!(
        unlock_with_password(&paths, "", WAIT).err(),
        Some(UnlockError::WrongPassword)
    );
    let _other = VaultLock::acquire(&paths, WAIT).unwrap();
    assert_eq!(
        unlock_with_password(&paths, "demo", WAIT).err(),
        Some(UnlockError::Stopped(StopReason::Busy))
    );
}

#[test]
fn a_newer_document_inside_a_current_file_stops() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    drop(create_vault(&paths, &keystore, WAIT).unwrap());
    let stored = keystore.load(KEYSTORE_ACCOUNT).unwrap().unwrap();
    let (key_id, key) = super::split_stored_key(&stored).unwrap();
    // A document whose own format number (key 0) is newer.
    let mut newer_doc = Vec::new();
    ciborium::ser::into_writer(
        &ciborium::Value::Map(vec![(
            ciborium::Value::Integer(0.into()),
            ciborium::Value::Integer(9.into()),
        )]),
        &mut newer_doc,
    )
    .unwrap();
    let header = SealedHeader::new(FileKind::Vault, KeySource::Keystore, key_id);
    fs::write(paths.vault(), seal(&header, &key, &newer_doc).unwrap()).unwrap();

    assert_eq!(
        stopped(start(&paths, &keystore, WAIT)),
        StopReason::NewerFormat(9)
    );
}

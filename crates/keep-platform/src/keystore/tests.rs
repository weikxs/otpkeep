use super::{Keystore, KeystoreError, MemoryKeystore};

#[test]
fn a_missing_secret_loads_as_none() {
    let store = MemoryKeystore::new();

    assert_eq!(store.load("data-key-1").unwrap(), None);
}

#[test]
fn stores_loads_and_deletes_a_secret() {
    let store = MemoryKeystore::new();

    store.store("data-key-1", &[7; 32]).unwrap();
    let loaded = store.load("data-key-1").unwrap();
    store.delete("data-key-1").unwrap();

    assert_eq!(loaded.as_deref().map(Vec::as_slice), Some(&[7u8; 32][..]));
    assert_eq!(store.load("data-key-1").unwrap(), None);
}

#[test]
fn storing_again_replaces_the_secret() {
    let store = MemoryKeystore::new();

    store.store("k", &[1; 4]).unwrap();
    store.store("k", &[2; 4]).unwrap();

    assert_eq!(
        store.load("k").unwrap().as_deref().cloned(),
        Some(vec![2; 4])
    );
}

#[test]
fn deleting_a_missing_secret_is_not_an_error() {
    assert_eq!(MemoryKeystore::new().delete("never-stored"), Ok(()));
}

#[test]
fn an_unavailable_store_reports_it_on_every_call() {
    let store = MemoryKeystore::unavailable();

    assert_eq!(store.load("k").err(), Some(KeystoreError::Unavailable));
    assert_eq!(
        store.store("k", &[1]).err(),
        Some(KeystoreError::Unavailable)
    );
    assert_eq!(store.delete("k").err(), Some(KeystoreError::Unavailable));
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod system {
    use super::super::{Keystore, KeystoreError, SystemKeystore, from_platform};

    #[test]
    fn platform_errors_never_carry_secret_bytes() {
        let secret = b"SECRET-BYTES-MUST-NOT-LEAK".to_vec();
        let error = from_platform(keyring_core::Error::BadEncoding(secret));

        assert_eq!(error, KeystoreError::Failed);
        assert!(!format!("{error:?} {error}").contains("SECRET"));
    }

    #[test]
    fn a_missing_entry_is_none_not_an_error() {
        assert_eq!(
            from_platform(keyring_core::Error::NoEntry),
            KeystoreError::NotFound
        );
    }

    /// Writes one test item to the real system keystore and removes it again. Run by hand only:
    /// `cargo test -p keep-platform -- --ignored`
    #[test]
    #[ignore = "uses the real system keystore"]
    fn round_trips_through_the_real_system_keystore() {
        let store = SystemKeystore::with_service("io.github.weikxs.otpkeep.test");
        let account = format!("probe-{}", std::process::id());

        store.store(&account, &[9; 32]).unwrap();
        let loaded = store.load(&account).unwrap();
        store.delete(&account).unwrap();

        assert_eq!(loaded.as_deref().cloned(), Some(vec![9; 32]));
        assert_eq!(store.load(&account).unwrap(), None);
    }
}

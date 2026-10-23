use std::time::{Duration, Instant};

use super::{LockError, VaultLock};
use crate::store::{VaultPaths, test_dir};

const SHORT: Duration = Duration::from_millis(300);

#[test]
fn one_holder_at_a_time() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());

    let first = VaultLock::acquire(&paths, SHORT).unwrap();
    let started = Instant::now();
    let second = VaultLock::acquire(&paths, SHORT);

    assert_eq!(second.err(), Some(LockError::Busy));
    assert!(
        started.elapsed() >= SHORT,
        "it should wait before giving up"
    );
    drop(first);
}

#[test]
fn the_lock_is_free_again_after_release() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());

    drop(VaultLock::acquire(&paths, SHORT).unwrap());

    assert!(VaultLock::acquire(&paths, SHORT).is_ok());
}

#[test]
fn a_waiting_holder_gets_the_lock_when_it_is_released() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    let first = VaultLock::acquire(&paths, SHORT).unwrap();

    let waiter = {
        let paths = paths.clone();
        std::thread::spawn(move || VaultLock::acquire(&paths, Duration::from_secs(5)).map(drop))
    };
    std::thread::sleep(Duration::from_millis(200));
    drop(first);

    assert_eq!(waiter.join().unwrap(), Ok(()));
}

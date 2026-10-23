use std::fs;

use super::{VaultPaths, replace_vault};
use crate::store::test_dir;

#[test]
fn the_first_write_creates_the_vault_without_a_previous_copy() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());

    replace_vault(&paths, b"first").unwrap();

    assert_eq!(fs::read(paths.vault()).unwrap(), b"first");
    assert!(!paths.prev().exists());
    assert!(!paths.tmp().exists());
}

#[test]
fn a_later_write_keeps_the_previous_version() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());

    replace_vault(&paths, b"first").unwrap();
    replace_vault(&paths, b"second").unwrap();
    replace_vault(&paths, b"third").unwrap();

    assert_eq!(fs::read(paths.vault()).unwrap(), b"third");
    assert_eq!(fs::read(paths.prev()).unwrap(), b"second");
}

#[test]
fn a_failed_write_leaves_the_vault_untouched() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    replace_vault(&paths, b"good").unwrap();
    // Make the temporary path unusable: a directory cannot be opened as a file.
    fs::create_dir(paths.tmp()).unwrap();

    let result = replace_vault(&paths, b"never written");

    assert!(result.is_err());
    assert_eq!(fs::read(paths.vault()).unwrap(), b"good");
}

#[test]
fn a_crash_before_the_final_rename_leaves_the_old_vault_readable() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    replace_vault(&paths, b"old").unwrap();
    // A crash after writing the temporary file but before renaming it.
    fs::write(paths.tmp(), b"half written").unwrap();

    assert_eq!(fs::read(paths.vault()).unwrap(), b"old");
    replace_vault(&paths, b"new").unwrap();
    assert_eq!(fs::read(paths.vault()).unwrap(), b"new");
    assert!(!paths.tmp().exists());
}

#[cfg(unix)]
#[test]
fn files_and_the_directory_are_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    use super::write_private;
    let dir = test_dir();
    let paths = VaultPaths::new(&dir.path().join("data"));

    replace_vault(&paths, b"x").unwrap();
    replace_vault(&paths, b"y").unwrap();
    write_private(&paths.local(), b"local").unwrap();

    let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(paths.dir()), 0o700);
    assert_eq!(mode(&paths.vault()), 0o600);
    assert_eq!(mode(&paths.prev()), 0o600);
    assert_eq!(mode(&paths.local()), 0o600);
}

#[test]
fn finds_vault_files_written_by_newer_versions() {
    let dir = test_dir();
    let paths = VaultPaths::new(dir.path());
    replace_vault(&paths, b"v1").unwrap();

    assert_eq!(paths.newer_vault_versions().unwrap(), Vec::<u32>::new());
    fs::write(dir.path().join("vault-v3.okv"), b"x").unwrap();
    fs::write(dir.path().join("vault-v2.okv"), b"x").unwrap();
    fs::write(dir.path().join("vault-vX.okv"), b"x").unwrap();
    assert_eq!(paths.newer_vault_versions().unwrap(), vec![2, 3]);
}

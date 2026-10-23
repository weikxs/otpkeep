use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use keep_vault::edit::{Change, NewEntry, add_entry, edit_entry};
use keep_vault::sealed::{
    DataKey, FileKind, KdfParams, KeySource, SEALED_FORMAT, SealedHeader, seal,
};
use keep_vault::{DeviceTag, Extra, Hlc, VaultDoc};

use super::{BackupError, read_backup, write_backup};

fn quick() -> KdfParams {
    KdfParams {
        salt: [5; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

fn vault_with_two_entries() -> VaultDoc {
    let empty = VaultDoc {
        vault_id: [0; 16],
        entries: vec![],
        tombstones: vec![],
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([1; 8]),
        extra: Extra::new(),
    };
    let hotp = CodeSpec::new(CodeKind::CounterBased, Digest::Sha256, 8).unwrap();
    let first = NewEntry {
        title: "Cloud (work)".into(),
        issuer: "Example Cloud".into(),
        account: "alex@example.com".into(),
        secret: SecretKey::from_bytes(vec![1; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: "恢复码在抽屉里".into(),
        tags: vec!["work".into()],
        keep_duplicate: false,
    };
    let second = NewEntry {
        title: "Git".into(),
        secret: SecretKey::from_bytes(vec![2; 20]).unwrap(),
        spec: hotp,
        counter: 9,
        ..first.clone()
    };
    let (doc, _) = add_entry(&empty, first, 1).unwrap();
    let (doc, id) = add_entry(&doc, second, 2).unwrap();
    edit_entry(&doc, id, Change::Note("second note".into()), 3).unwrap()
}

#[test]
fn a_backup_restores_names_notes_tags_and_keys() {
    let doc = vault_with_two_entries();
    let entries: Vec<_> = doc.entries.iter().collect();

    let bytes = write_backup(&entries, "备份口令 demo", quick()).unwrap();
    let restored = read_backup(&bytes, "备份口令 demo").unwrap();

    assert_eq!(restored.len(), 2);
    for (original, back) in doc.entries.iter().zip(&restored) {
        assert_eq!(back.title, original.title.value);
        assert_eq!(back.issuer, original.issuer.value);
        assert_eq!(back.account, original.account.value);
        assert_eq!(back.note, original.note.value);
        assert_eq!(back.tags, original.tags.value);
        assert_eq!(back.secret, original.key.value.secret);
        assert_eq!(
            (back.spec, back.counter),
            (original.key.value.spec, original.key.value.counter)
        );
    }
}

#[test]
fn a_backup_does_not_contain_anything_readable() {
    let doc = vault_with_two_entries();
    let entries: Vec<_> = doc.entries.iter().collect();

    let bytes = write_backup(&entries, "pw", quick()).unwrap();

    for needle in [
        &b"alex@example.com"[..],
        b"Example Cloud",
        "抽屉".as_bytes(),
    ] {
        assert!(!bytes.windows(needle.len()).any(|w| w == needle));
    }
}

#[test]
fn the_wrong_password_is_reported() {
    let doc = vault_with_two_entries();
    let bytes = write_backup(&doc.entries.iter().collect::<Vec<_>>(), "right", quick()).unwrap();

    assert_eq!(
        read_backup(&bytes, "wrong").err(),
        Some(BackupError::WrongPassword)
    );
}

#[test]
fn an_empty_password_is_refused_both_ways() {
    let doc = vault_with_two_entries();

    assert_eq!(
        write_backup(&doc.entries.iter().collect::<Vec<_>>(), "", quick()).err(),
        Some(BackupError::EmptyPassword)
    );
    let bytes = write_backup(&doc.entries.iter().collect::<Vec<_>>(), "pw", quick()).unwrap();
    assert_eq!(
        read_backup(&bytes, "").err(),
        Some(BackupError::EmptyPassword)
    );
}

#[test]
fn other_files_are_not_backups() {
    let vault_file = seal(
        &SealedHeader::new(FileKind::Vault, KeySource::Keystore, [0; 16]),
        &DataKey::from_bytes([0; 32]),
        b"x",
    )
    .unwrap();

    assert_eq!(
        read_backup(b"PK\x03\x04zip", "pw").err(),
        Some(BackupError::NotABackup)
    );
    assert_eq!(
        read_backup(&vault_file, "pw").err(),
        Some(BackupError::NotABackup)
    );
}

#[test]
fn a_backup_from_a_newer_version_says_so() {
    let header = SealedHeader {
        format: SEALED_FORMAT + 1,
        kdf: Some(quick()),
        ..SealedHeader::new(FileKind::Backup, KeySource::MasterPassword, [0; 16])
    };
    let bytes = seal(&header, &DataKey::from_bytes([0; 32]), b"x").unwrap();

    assert_eq!(
        read_backup(&bytes, "pw").err(),
        Some(BackupError::NewerFormat)
    );
}

#[test]
fn absurd_derivation_parameters_are_refused_without_trying() {
    let header = SealedHeader {
        kdf: Some(KdfParams {
            m_cost_kib: u32::MAX,
            ..quick()
        }),
        ..SealedHeader::new(FileKind::Backup, KeySource::MasterPassword, [0; 16])
    };
    let bytes = seal(&header, &DataKey::from_bytes([0; 32]), b"x").unwrap();

    assert_eq!(read_backup(&bytes, "pw").err(), Some(BackupError::Damaged));
}

#[test]
fn every_backup_is_encrypted_differently() {
    let doc = vault_with_two_entries();
    let entries: Vec<_> = doc.entries.iter().collect();

    assert_ne!(
        write_backup(&entries, "pw", quick()).unwrap(),
        write_backup(&entries, "pw", quick()).unwrap()
    );
}

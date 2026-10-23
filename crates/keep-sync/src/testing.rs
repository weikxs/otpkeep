//! Shared test fixtures: vaults on devices of one group, built with the vault's own edit functions.

use std::collections::BTreeMap;

use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use keep_vault::edit::{NewEntry, add_entry};
use keep_vault::{DeviceId, DeviceTag, EntryId, Extra, Hlc, SyncGroup, VaultDoc};

pub(crate) const GROUP: [u8; 16] = [0x42; 16];
pub(crate) const MINUTE_MS: u64 = 60 * 1000;
pub(crate) const DAY_MS: u64 = 24 * 60 * MINUTE_MS;

/// An empty vault in the test group on device `n`.
pub(crate) fn device_doc(n: u8) -> VaultDoc {
    VaultDoc {
        vault_id: [n; 16],
        entries: vec![],
        tombstones: vec![],
        group: Some(SyncGroup {
            group_id: GROUP,
            tag_key: [9; 32],
            events: vec![],
            presence: BTreeMap::new(),
            extra: Extra::new(),
        }),
        device: None,
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([n; 8]),
        extra: Extra::new(),
    }
}

/// The same synced data as `doc`, as held by device `n`.
pub(crate) fn on_device(doc: &VaultDoc, n: u8) -> VaultDoc {
    VaultDoc {
        vault_id: [n; 16],
        device_tag: DeviceTag([n; 8]),
        ..doc.clone()
    }
}

pub(crate) fn device_id(n: u8) -> DeviceId {
    DeviceId([n; 32])
}

pub(crate) fn secret(byte: u8) -> SecretKey {
    SecretKey::from_bytes(vec![byte; 20]).unwrap()
}

pub(crate) fn hotp() -> CodeSpec {
    CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap()
}

pub(crate) fn new_entry(title: &str, byte: u8) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "demo-user".to_owned(),
        secret: secret(byte),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: vec![],
        keep_duplicate: false,
    }
}

pub(crate) fn add(doc: &VaultDoc, title: &str, byte: u8, now_ms: u64) -> (VaultDoc, EntryId) {
    add_entry(doc, new_entry(title, byte), now_ms).unwrap()
}

pub(crate) fn entry(doc: &VaultDoc, id: EntryId) -> &keep_vault::Entry {
    doc.entries.iter().find(|e| e.id == id).unwrap()
}

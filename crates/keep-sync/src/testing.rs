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

/// Device `n`'s identity in the test group.
pub(crate) fn identity(n: u8) -> crate::identity::DeviceIdentity {
    crate::identity::DeviceIdentity::from_seeds(&[n; 32], &[n.wrapping_add(100); 32], &GROUP)
}

/// Events in which device `founder` founds the group and adds `others`.
pub(crate) fn founded(founder: u8, others: &[u8]) -> Vec<keep_vault::MemberEvent> {
    let author = identity(founder);
    let join = |n: u8, wall_ms: u64| {
        let joined = identity(n);
        crate::membership::sign_event(
            &author,
            &GROUP,
            keep_vault::MemberKind::Join {
                device: joined.device_id(),
                dx_public: joined.dx_public(),
                dx_cert: joined.dx_cert(),
                platform: keep_vault::Platform::Linux,
            },
            Hlc {
                wall_ms,
                logical: 0,
                device: DeviceTag([founder; 8]),
            },
        )
    };
    std::iter::once(join(founder, 1))
        .chain(others.iter().zip(2..).map(|(n, t)| join(*n, t)))
        .collect()
}

/// Device `n` of a group of devices 1, 2 and 3, holding `events`.
pub(crate) fn member_doc(n: u8, events: &[keep_vault::MemberEvent]) -> VaultDoc {
    let mut doc = device_doc(n);
    doc.device_tag = crate::identity::device_tag(identity(n).device_id());
    if let Some(group) = doc.group.as_mut() {
        group.events = events.to_vec();
    }
    doc
}

/// A Noise channel between device 1 (opener) and device 2 (joiner), over an in-memory pipe.
pub(crate) async fn channel_pair() -> (
    crate::channel::noise::Channel<tokio::io::DuplexStream>,
    crate::channel::noise::Channel<tokio::io::DuplexStream>,
) {
    use crate::channel::hello::Hello;
    use crate::channel::noise::{Purpose, accept_noise, connect_noise};
    let (a, b) = tokio::io::duplex(1 << 16);
    let (opener, joiner) = (identity(1), identity(2));
    let hello = |d: &crate::identity::DeviceIdentity| Hello {
        device: d.device_id(),
        dx_cert: Some(d.dx_cert()),
        platform: keep_vault::Platform::Linux,
        lan_name: "Test".to_owned(),
        versions: vec![crate::channel::PROTOCOL_VERSION],
    };
    let (opener_hello, joiner_hello) = (hello(&opener), hello(&joiner));
    let (open, join) = tokio::join!(
        accept_noise(
            a,
            Purpose::Sync,
            &[1; 16],
            opener.exchange_secret(),
            &opener_hello
        ),
        connect_noise(
            b,
            Purpose::Sync,
            &[1; 16],
            joiner.exchange_secret(),
            &joiner_hello
        )
    );
    (open.unwrap().0, join.unwrap().0)
}

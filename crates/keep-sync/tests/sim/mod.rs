//! The simulated world: devices, their clocks, and what the checks need to remember.

mod ops;

use std::collections::{BTreeMap, BTreeSet};

use keep_sync::identity::{DeviceIdentity, device_tag};
use keep_sync::membership::{Membership, membership, merge_events, sign_event};
use keep_sync::session::{SyncStop, sync_data};
use keep_vault::{
    DeviceId, DeviceTag, EntryId, Extra, Hlc, MemberEvent, MemberKind, Platform, SyncGroup,
    VaultDoc,
};

const GROUP: [u8; 16] = [0x5a; 16];
const MIN_DEVICES: u64 = 3;
const MAX_EXTRA_DEVICES: u64 = 14;
const START_MS: u64 = 1_700_000_000_000;
const MINUTE_MS: u64 = 60 * 1000;
const DAY_MS: u64 = 24 * 60 * MINUTE_MS;
/// Clocks are off by up to this much either way.
const MAX_SKEW_MS: i64 = 10 * 60 * 1000;
/// Percentage of sessions that break off before committing.
const DISCONNECT_PERCENT: u64 = 10;
const SETTLE_ROUNDS: usize = 3;

/// A small deterministic random source (`SplitMix64`), so a failing seed can be replayed exactly.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` (`n` > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap()
    }

    pub fn percent(&mut self, p: u64) -> bool {
        self.below(100) < p
    }

    pub fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for chunk in out.chunks_mut(8) {
            let word = self.next().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        out
    }
}

pub struct Device {
    pub identity: DeviceIdentity,
    pub doc: VaultDoc,
    /// How far this device's clock is off.
    pub skew_ms: i64,
}

impl Device {
    pub fn id(&self) -> DeviceId {
        self.identity.device_id()
    }
}

pub struct World {
    pub devices: Vec<Device>,
    /// The true time.
    pub now_ms: u64,
    /// Every entry that ever held each key.
    pub holders: BTreeMap<Vec<u8>, BTreeSet<EntryId>>,
    /// Entries a user deleted for good (or whose trash time ran out).
    pub deleted: BTreeSet<EntryId>,
    /// Entries whose tombstones were cleared.
    pub pruned: BTreeSet<EntryId>,
    /// Who cleared each tombstone, and the members it saw (diagnostics).
    pub prune_log: BTreeMap<EntryId, (usize, Hlc)>,
    /// Counter for new keys and entry ids.
    pub serial: u32,
    /// Every membership event any device holds, and the members they give.
    pub all_events: Vec<MemberEvent>,
    pub known: Membership,
}

impl World {
    pub fn new(rng: &mut Rng) -> Self {
        let count = MIN_DEVICES + rng.below(MAX_EXTRA_DEVICES);
        let identities: Vec<DeviceIdentity> = (0..count)
            .map(|_| DeviceIdentity::from_seeds(&rng.bytes(), &rng.bytes(), &GROUP))
            .collect();
        let founder = &identities[0];
        let events: Vec<MemberEvent> = identities
            .iter()
            .zip(1u64..)
            .map(|(joined, n)| {
                sign_event(
                    founder,
                    &GROUP,
                    MemberKind::Join {
                        device: joined.device_id(),
                        dx_public: joined.dx_public(),
                        dx_cert: joined.dx_cert(),
                        platform: Platform::Linux,
                    },
                    Hlc {
                        wall_ms: START_MS - DAY_MS + n,
                        logical: 0,
                        device: device_tag(founder.device_id()),
                    },
                )
            })
            .collect();
        let devices = identities
            .into_iter()
            .zip(0u8..)
            .map(|(identity, n)| {
                let skew_ms =
                    i64::try_from(rng.below(2 * MAX_SKEW_MS.unsigned_abs())).unwrap() - MAX_SKEW_MS;
                Device {
                    doc: empty_doc(n, &identity, &events),
                    identity,
                    skew_ms,
                }
            })
            .collect();
        Self {
            devices,
            now_ms: START_MS,
            holders: BTreeMap::new(),
            deleted: BTreeSet::new(),
            pruned: BTreeSet::new(),
            prune_log: BTreeMap::new(),
            serial: 0,
            known: membership(&group_with(&events)),
            all_events: events,
        }
    }

    /// Records an event a device just signed.
    pub fn note_event(&mut self, event: MemberEvent) {
        self.all_events = merge_events(&GROUP, &self.all_events, &[event]);
        self.known = membership(&group_with(&self.all_events));
    }

    /// One random action.
    pub fn step(&mut self, rng: &mut Rng, seed: u64) {
        self.now_ms += rng.below(5 * MINUTE_MS);
        match rng.below(100) {
            0..=34 => self.sync_random_pair(rng, seed),
            35..=37 => self.now_ms += rng.below(40 * DAY_MS),
            38..=39 => self.step_clock_back(rng),
            40..=41 => self.prune(rng),
            _ => {
                let index = rng.index(self.devices.len());
                ops::edit(self, index, rng);
            }
        }
        self.record_keys();
        self.check_pruned_stay_gone(seed);
    }

    fn step_clock_back(&mut self, rng: &mut Rng) {
        let index = rng.index(self.devices.len());
        let device = &mut self.devices[index];
        let back = i64::try_from(rng.below(MAX_SKEW_MS.unsigned_abs())).unwrap();
        device.skew_ms = (device.skew_ms - back).max(-MAX_SKEW_MS);
    }

    /// The device's own clock.
    pub fn local_now(&self, index: usize) -> u64 {
        self.now_ms
            .saturating_add_signed(self.devices[index].skew_ms)
    }

    /// Membership as every event any device holds says.
    pub fn known_membership(&self) -> &Membership {
        &self.known
    }

    fn sync_random_pair(&mut self, rng: &mut Rng, seed: u64) {
        let opener = rng.index(self.devices.len());
        let joiner = rng.index(self.devices.len());
        if opener == joiner {
            return;
        }
        let disconnect = rng.percent(DISCONNECT_PERCENT);
        self.sync_pair(opener, joiner, disconnect, seed);
    }

    /// Syncs two devices; a disconnect drops the result, as the real session does.
    pub fn sync_pair(&mut self, opener: usize, joiner: usize, disconnect: bool, seed: u64) {
        let (a, b) = (&self.devices[opener], &self.devices[joiner]);
        let pair_events = merge_events(&GROUP, &group(&a.doc).events, &group(&b.doc).events);
        let pair_view = membership(&SyncGroup {
            events: pair_events,
            ..group(&a.doc).clone()
        });
        let result = sync_data(
            &a.doc,
            &b.doc,
            a.id(),
            b.id(),
            self.local_now(opener),
            self.local_now(joiner),
        );
        let either_removed = pair_view.is_removed(a.id()) || pair_view.is_removed(b.id());
        match result {
            Ok(done) => {
                assert!(
                    !either_removed,
                    "seed {seed}: a removed device completed a sync (property 5)"
                );
                if !disconnect {
                    self.devices[opener].doc = done.opener;
                    self.devices[joiner].doc = done.joiner;
                }
            }
            Err(SyncStop::Removed(_)) => assert!(either_removed, "seed {seed}: wrongly refused"),
            Err(other) => panic!("seed {seed}: sync stopped: {other:?}"),
        }
    }

    fn prune(&mut self, rng: &mut Rng) {
        let index = rng.index(self.devices.len());
        let device = &self.devices[index];
        let view = membership(group(&device.doc));
        let members: BTreeSet<DeviceId> = view.members.keys().copied().collect();
        let now = self.local_now(index);
        let before: BTreeSet<EntryId> = device.doc.tombstones.iter().map(|t| t.entry).collect();
        let after = keep_sync::housekeeping::prune_tombstones(&device.doc, &members, now);
        let kept: BTreeSet<EntryId> = after.tombstones.iter().map(|t| t.entry).collect();
        for tomb in &device.doc.tombstones {
            if kept.contains(&tomb.entry) {
                continue;
            }
            let everyone = members.iter().all(|m| tomb.acked_by.contains(&m.0));
            assert!(
                everyone,
                "a tombstone was cleared before every member held it (property 4)"
            );
            assert!(
                now.saturating_sub(tomb.stamp.wall_ms)
                    > keep_sync::housekeeping::TOMBSTONE_GRACE_MS,
                "a tombstone was cleared early (property 4)"
            );
        }
        self.pruned.extend(before.difference(&kept));
        for tomb in device
            .doc
            .tombstones
            .iter()
            .filter(|t| !kept.contains(&t.entry))
        {
            self.prune_log.insert(tomb.entry, (index, tomb.stamp));
        }
        self.devices[index].doc = after;
    }

    /// Remembers which entries hold which keys, on every device.
    pub fn record_keys(&mut self) {
        for device in &self.devices {
            for entry in &device.doc.entries {
                self.holders
                    .entry(entry.key.value.secret.expose().to_vec())
                    .or_default()
                    .insert(entry.id);
            }
        }
    }

    /// An entry whose tombstone was cleared may only reappear with a change made after its deletion: that is
    /// a concurrent edit winning over the deletion (design/06-sync.md §6.2), not a resurrection.
    fn check_pruned_stay_gone(&self, seed: u64) {
        let members = self.known_membership();
        for (index, device) in self.devices.iter().enumerate() {
            if !members.is_member(device.id()) {
                continue;
            }
            for entry in &device.doc.entries {
                if let Some((pruner, deleted_at)) = self.prune_log.get(&entry.id) {
                    assert!(
                        newest_field(entry) > *deleted_at,
                        "seed {seed}: entry {:?} came back on device {index} after device {pruner} cleared \
                         its tombstone (property 4)",
                        entry.id
                    );
                }
            }
        }
    }

    /// Stops activity and passes changes along a chain of the members and back, several times.
    pub fn settle(&mut self, seed: u64) {
        self.now_ms += MINUTE_MS;
        for _ in 0..SETTLE_ROUNDS {
            let members = self.member_indexes();
            let forward = members.windows(2).map(|w| (w[0], w[1]));
            let backward = members.windows(2).rev().map(|w| (w[1], w[0]));
            let pairs: Vec<(usize, usize)> = forward.chain(backward).collect();
            for (opener, joiner) in pairs {
                self.now_ms += 1_000;
                self.sync_pair(opener, joiner, false, seed);
            }
        }
        self.record_keys();
    }

    pub fn member_indexes(&self) -> Vec<usize> {
        let members = self.known_membership();
        (0..self.devices.len())
            .filter(|i| members.is_member(self.devices[*i].id()))
            .collect()
    }

    pub fn check_final(&self, seed: u64) {
        let members = self.member_indexes();
        let digests: BTreeSet<[u8; 32]> = members
            .iter()
            .map(|i| keep_sync::digest::state_digest(&self.devices[*i].doc))
            .collect();
        assert_eq!(
            digests.len(),
            1,
            "seed {seed}: members disagree (property 1)"
        );
        let doc = &self.devices[members[0]].doc;
        for (key, holders) in &self.holders {
            // Keys only a removed device holds are still on that device.
            let present = self.devices.iter().any(|d| {
                d.doc
                    .entries
                    .iter()
                    .any(|e| e.key.value.secret.expose() == key.as_slice())
            });
            // A replaced key moves to an old-key copy, so a key is accounted for once any entry that held it
            // was deleted by a user.
            if !present && holders.is_disjoint(&self.deleted) {
                let missing: Vec<_> = holders.difference(&self.deleted).collect();
                let tombstoned: Vec<_> = missing
                    .iter()
                    .filter(|id| doc.tombstones.iter().any(|t| t.entry == ***id))
                    .collect();
                panic!(
                    "seed {seed}: a key vanished without being deleted (property 2): holders {holders:?}, \
                     not deleted {missing:?}, tombstoned {tombstoned:?}, pruned {:?}",
                    missing
                        .iter()
                        .filter(|id| self.pruned.contains(**id))
                        .collect::<Vec<_>>()
                );
            }
            let outside_trash = doc
                .entries
                .iter()
                .filter(|e| {
                    e.key.value.secret.expose() == key.as_slice()
                        && e.trashed_at.value.is_none()
                        && !e.keep_duplicate.value
                })
                .count();
            assert!(
                outside_trash <= 1,
                "seed {seed}: a duplicate key survived (property 3)"
            );
        }
    }
}

fn empty_doc(n: u8, identity: &DeviceIdentity, events: &[MemberEvent]) -> VaultDoc {
    VaultDoc {
        vault_id: [n; 16],
        entries: vec![],
        tombstones: vec![],
        group: Some(SyncGroup {
            group_id: GROUP,
            tag_key: [7; 32],
            events: events.to_vec(),
            presence: BTreeMap::new(),
            extra: Extra::new(),
        }),
        device: Some(identity.secrets()),
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: device_tag(identity.device_id()),
        extra: Extra::new(),
    }
}

impl World {
    /// A key never used before.
    pub fn new_secret(&mut self) -> keep_passcode::SecretKey {
        self.serial += 1;
        let mut bytes = vec![0x5c; 20];
        bytes[..4].copy_from_slice(&self.serial.to_be_bytes());
        keep_passcode::SecretKey::from_bytes(bytes).unwrap()
    }

    /// An entry id never used before.
    pub fn new_entry_id(&mut self) -> EntryId {
        self.serial += 1;
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&self.serial.to_be_bytes());
        EntryId(id)
    }
}

fn newest_field(entry: &keep_vault::Entry) -> Hlc {
    [
        entry.created_at,
        entry.title.stamp,
        entry.issuer.stamp,
        entry.account.stamp,
        entry.key.stamp,
        entry.note.stamp,
        entry.tags.stamp,
        entry.rank.stamp,
        entry.pinned.stamp,
        entry.keep_duplicate.stamp,
        entry.trashed_at.stamp,
        entry.old_key_of.stamp,
    ]
    .into_iter()
    .max()
    .unwrap()
}

fn group_with(events: &[MemberEvent]) -> SyncGroup {
    SyncGroup {
        group_id: GROUP,
        tag_key: [7; 32],
        events: events.to_vec(),
        presence: BTreeMap::new(),
        extra: Extra::new(),
    }
}

pub fn group(doc: &VaultDoc) -> &SyncGroup {
    doc.group.as_ref().unwrap()
}

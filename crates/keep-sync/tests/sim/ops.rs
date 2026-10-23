//! The edits a person makes on one device.

use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use keep_sync::membership::sign_event;
use keep_vault::edit::{
    Change, NewEntry, Placement, add_entry_with_id, edit_entry, move_entry, move_to_trash,
    next_hotp, purge, purge_expired_trash, replace_key, restore,
};
use keep_vault::{EntryId, Hlc, MemberKind, VaultDoc};

use super::{GROUP, Rng, World, group};

/// Percentage of new entries that reuse a key already seen, to make duplicates.
const REUSE_KEY_PERCENT: u64 = 15;
const HOTP_PERCENT: u64 = 25;

pub fn edit(world: &mut World, index: usize, rng: &mut Rng) {
    let now = world.local_now(index);
    let doc = world.devices[index].doc.clone();
    let target = pick_entry(&doc, rng);
    let next = match (rng.below(16), target) {
        (0..=3, _) | (_, None) => add(world, &doc, rng, now),
        (4, Some(id)) => edit_entry(
            &doc,
            id,
            Change::Title(format!("Title {}", rng.below(1000))),
            now,
        )
        .ok(),
        (5, Some(id)) => edit_entry(
            &doc,
            id,
            Change::Note(format!("Note {}", rng.below(1000))),
            now,
        )
        .ok(),
        (6, Some(id)) => edit_entry(
            &doc,
            id,
            Change::Tags(vec![format!("t{}", rng.below(5))]),
            now,
        )
        .ok(),
        (7, Some(id)) => edit_entry(&doc, id, Change::Pinned(rng.percent(50)), now).ok(),
        (8, Some(id)) => {
            let secret = world.new_secret();
            replace_key(&doc, id, secret, spec(rng), 0, now).ok()
        }
        (9, Some(id)) => next_hotp(&doc, id, now).ok().map(|(doc, _)| doc),
        (10, Some(id)) => move_to_trash(&doc, id, now).ok(),
        (11, Some(id)) => restore(&doc, id, now).ok(),
        (12, Some(id)) => {
            let purged = purge(&doc, id, now).ok();
            if purged.is_some() {
                world.deleted.insert(id);
            }
            purged
        }
        (13, _) => {
            let expired = purge_expired_trash(&doc, now);
            world.deleted.extend(
                doc.entries
                    .iter()
                    .map(|e| e.id)
                    .filter(|id| !expired.entries.iter().any(|e| e.id == *id)),
            );
            Some(expired)
        }
        (14, Some(id)) => move_entry(&doc, id, placement(&doc, rng), now).ok(),
        (_, Some(_)) => {
            let (doc, event) = membership_change(world, index, rng, now);
            world.note_event(event);
            Some(doc)
        }
    };
    if let Some(next) = next {
        world.devices[index].doc = next;
    }
}

fn add(world: &mut World, doc: &VaultDoc, rng: &mut Rng, now: u64) -> Option<VaultDoc> {
    let reuse = rng.percent(REUSE_KEY_PERCENT) && !world.holders.is_empty();
    let secret = if reuse {
        let keys: Vec<&Vec<u8>> = world.holders.keys().collect();
        SecretKey::from_bytes(keys[rng.index(keys.len())].clone()).ok()?
    } else {
        world.new_secret()
    };
    let new = NewEntry {
        title: format!("Account {}", rng.below(1000)),
        issuer: "Example".to_owned(),
        account: "demo-user".to_owned(),
        secret,
        spec: spec(rng),
        counter: 0,
        note: String::new(),
        tags: vec![],
        keep_duplicate: reuse && rng.percent(50),
    };
    let id = world.new_entry_id();
    add_entry_with_id(doc, new, id, now)
        .ok()
        .map(|(doc, _)| doc)
}

fn spec(rng: &mut Rng) -> CodeSpec {
    if rng.percent(HOTP_PERCENT) {
        CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap_or_default()
    } else {
        CodeSpec::default()
    }
}

fn pick_entry(doc: &VaultDoc, rng: &mut Rng) -> Option<EntryId> {
    if doc.entries.is_empty() {
        return None;
    }
    Some(doc.entries[rng.index(doc.entries.len())].id)
}

fn placement(doc: &VaultDoc, rng: &mut Rng) -> Placement {
    let other = doc.entries[rng.index(doc.entries.len())].id;
    match rng.below(4) {
        0 => Placement::First,
        1 => Placement::Last,
        2 => Placement::Before(other),
        _ => Placement::After(other),
    }
}

/// Renames this device, or removes another member while at least two remain.
fn membership_change(
    world: &World,
    index: usize,
    rng: &mut Rng,
    now: u64,
) -> (VaultDoc, keep_vault::MemberEvent) {
    let device = &world.devices[index];
    let members = world.member_indexes();
    let kind = if rng.percent(70) || members.len() <= 2 {
        MemberKind::Rename {
            device: device.id(),
            lan_name: format!("Device {}", rng.below(100)),
        }
    } else {
        let removed = members[rng.index(members.len())];
        MemberKind::Remove {
            device: world.devices[removed].id(),
        }
    };
    let stamp = Hlc {
        wall_ms: now.max(device.doc.clock.wall_ms),
        logical: device.doc.clock.logical.saturating_add(1),
        device: device.doc.device_tag,
    };
    let event = sign_event(&device.identity, &GROUP, kind, stamp);
    let mut doc = device.doc.clone();
    let mut grouped = group(&doc).clone();
    grouped.events.push(event.clone());
    doc.group = Some(grouped);
    doc.clock = doc.clock.max(stamp);
    (doc, event)
}

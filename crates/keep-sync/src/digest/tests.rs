use keep_vault::edit::{Change, edit_entry};
use keep_vault::{DeviceTag, Hlc};

use super::{CLOCK_SKEW_LIMIT_MS, PeerClock, check_peer_clock, state_digest};
use crate::testing::{DAY_MS, MINUTE_MS, add, device_doc, on_device};

#[test]
fn the_digest_covers_synced_data_only() {
    let (a, id) = add(&device_doc(1), "Example Cloud", 1, MINUTE_MS);
    let b = on_device(&a, 2);
    let mut reordered = a.clone();
    reordered.entries.reverse();
    let edited = edit_entry(&a, id, Change::Note("n".into()), 2 * MINUTE_MS).unwrap();

    assert_eq!(
        state_digest(&a),
        state_digest(&b),
        "vault id and device tag are local"
    );
    assert_eq!(state_digest(&a), state_digest(&reordered));
    assert_ne!(state_digest(&a), state_digest(&edited));
}

#[test]
fn a_clock_more_than_a_day_ahead_is_refused() {
    let now = 10 * DAY_MS;
    let (ok, _) = add(&device_doc(1), "A", 1, now + CLOCK_SKEW_LIMIT_MS);
    let (ahead, _) = add(&device_doc(1), "A", 1, now + CLOCK_SKEW_LIMIT_MS + 1);
    let mut ahead_presence = device_doc(1);
    let stamp = Hlc {
        wall_ms: now + 2 * DAY_MS,
        logical: 0,
        device: DeviceTag([1; 8]),
    };
    ahead_presence
        .group
        .as_mut()
        .unwrap()
        .presence
        .insert(crate::testing::device_id(1), stamp);

    assert_eq!(check_peer_clock(&ok, now), Ok(()));
    assert!(check_peer_clock(&ahead, now).is_err());
    assert_eq!(
        check_peer_clock(&ahead_presence, now),
        Err(PeerClock { stamp })
    );
}

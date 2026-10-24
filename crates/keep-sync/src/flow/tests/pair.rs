//! Pairing sessions over TCP on loopback.

use std::sync::Arc;

use keep_vault::{Platform, VaultDoc};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::{CODE, lan_acceptor, loopback, quick};
use crate::channel::PROTOCOL_VERSION;
use crate::channel::code::Secret;
use crate::channel::hello::Hello;
use crate::channel::message::ErrorCode;
use crate::flow::listen::Listeners;
use crate::flow::pair_join::{JoinMode, JoinPairOutcome, PairTarget, join_pair};
use crate::flow::pair_open::{PairOutcome, PairWindow, run_pair_window};
use crate::flow::sync_join::{JoinOutcome, Joiner, join_sync};
use crate::flow::sync_open::{OpenerEvent, SyncWindow, WindowOutcome, run_sync_window};
use crate::group::{found_group, identity_of};
use crate::identity::{NewDevice, short_fingerprint};
use crate::lan::announce::{Announcement, FoundWindow};
use crate::lan::window::{CloseReason, WindowKind};
use crate::membership::membership;
use crate::testing::{MINUTE_MS, add, device_doc};

const PAIR_CODE: &str = "24681357";
const SECRET: [u8; 16] = [0x5e; 16];
const RANDOM: [u8; 16] = [0x31; 16];

fn alone(n: u8) -> VaultDoc {
    let mut doc = device_doc(n);
    doc.group = None;
    doc
}

fn hello(device: keep_vault::DeviceId, cert: Option<[u8; 64]>, name: &str) -> Hello {
    Hello {
        device,
        dx_cert: cert,
        platform: Platform::Linux,
        lan_name: name.to_owned(),
        versions: vec![PROTOCOL_VERSION],
    }
}

/// Device 1 with one entry, in a new group, opens a pairing window; its person answers the check code with
/// `confirm`.
async fn open_pairing(confirm: bool) -> (VaultDoc, u16, tokio::task::JoinHandle<PairOutcome>) {
    let (doc, _) = add(&alone(1), "On the Mac", 1, MINUTE_MS);
    let doc = found_group(&doc, Platform::MacOs, "Study Mac", MINUTE_MS).unwrap();
    let identity = identity_of(&doc).unwrap();
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let port = listeners.port();
    let window = PairWindow {
        me: identity.device_id(),
        exchange: identity.exchange_secret().clone(),
        hello: hello(identity.device_id(), Some(identity.dx_cert()), "Study Mac"),
        code: Zeroizing::new(PAIR_CODE.to_owned()),
        secret: Zeroizing::new(SECRET),
        window_random: RANDOM,
        wait_ms: 60_000,
        timers: quick(),
    };
    let (events, mut asks) = mpsc::channel(4);
    tokio::spawn(async move {
        while let Some(event) = asks.recv().await {
            if let OpenerEvent::CheckCode { answer, .. } = event {
                let _ = answer.send(confirm);
            }
        }
    });
    let current_doc = doc.clone();
    let current = Arc::new(move || current_doc.clone());
    let task = tokio::spawn(run_pair_window(
        window,
        lan_acceptor(listeners),
        (),
        current,
        Arc::new(|| 2 * MINUTE_MS),
        events,
    ));
    (doc, port, task)
}

fn target(port: u16, opener: &VaultDoc, secret: Secret) -> PairTarget {
    PairTarget {
        route: crate::transport::Route::Lan {
            addrs: vec![(std::net::IpAddr::from([127, 0, 0, 1]), None)],
            port,
        },
        window_random: RANDOM,
        short_fingerprint: short_fingerprint(identity_of(opener).unwrap().device_id()),
        secret,
    }
}

async fn join(target: &PairTarget, doc: &VaultDoc, decision: Option<JoinMode>) -> JoinPairOutcome {
    let new = NewDevice::generate().unwrap();
    let joiner_hello = hello(new.device_id(), None, "Phone");
    join_pair(
        target,
        new,
        &joiner_hello,
        doc,
        quick(),
        || 2 * MINUTE_MS,
        |_, _| async move { decision },
    )
    .await
}

#[tokio::test]
async fn scanning_the_qr_code_adds_the_device_and_merges_its_entries() {
    let (opener_doc, port, window) = open_pairing(true).await;
    let (phone, _) = add(&alone(2), "On the phone", 2, MINUTE_MS);

    let joined = join(
        &target(port, &opener_doc, Secret::PairSecret(SECRET)),
        &phone,
        Some(JoinMode::Merge),
    )
    .await;
    let opened = window.await.unwrap();

    let JoinPairOutcome::Paired {
        result: on_phone,
        replaced,
        ..
    } = joined
    else {
        panic!("joiner: {joined:?}");
    };
    let PairOutcome::Paired {
        result: on_mac,
        peer,
    } = opened
    else {
        panic!("opener: {opened:?}");
    };
    assert!(!replaced);
    assert_eq!(on_mac.digest, on_phone.digest);
    assert_eq!(on_phone.doc.entries.len(), 2);
    let members = membership(on_mac.doc.group.as_ref().unwrap());
    assert_eq!(members.members.len(), 2);
    assert_eq!(
        members.members[&peer.device].lan_name.as_deref(),
        Some("Phone")
    );
    assert_eq!(identity_of(&on_phone.doc).unwrap().device_id(), peer.device);
}

#[tokio::test]
async fn the_typed_code_works_too_and_replace_drops_the_joiners_entries() {
    let (opener_doc, port, window) = open_pairing(true).await;
    let (phone, _) = add(&alone(2), "Old phone entry", 2, MINUTE_MS);
    let typed = Secret::PairCode(Zeroizing::new(PAIR_CODE.to_owned()));

    let joined = join(
        &target(port, &opener_doc, typed),
        &phone,
        Some(JoinMode::Replace),
    )
    .await;

    let JoinPairOutcome::Paired {
        result, replaced, ..
    } = joined
    else {
        panic!("joiner: {joined:?}");
    };
    assert!(replaced);
    assert_eq!(result.doc.entries.len(), 1, "only the group's entry");
    assert!(matches!(window.await.unwrap(), PairOutcome::Paired { .. }));
}

#[tokio::test]
async fn a_check_code_mismatch_on_either_side_cancels() {
    let (opener_doc, port, window) = open_pairing(true).await;
    let declined = join(
        &target(port, &opener_doc, Secret::PairSecret(SECRET)),
        &alone(2),
        None,
    )
    .await;
    assert!(
        matches!(declined, JoinPairOutcome::Stopped(ErrorCode::SasMismatch)),
        "{declined:?}"
    );
    assert!(matches!(
        window.await.unwrap(),
        PairOutcome::Stopped(ErrorCode::SasMismatch)
    ));

    let (opener_doc, port, window) = open_pairing(false).await;
    let refused = join(
        &target(port, &opener_doc, Secret::PairSecret(SECRET)),
        &alone(2),
        Some(JoinMode::Merge),
    )
    .await;
    assert!(
        matches!(refused, JoinPairOutcome::Stopped(ErrorCode::SasMismatch)),
        "{refused:?}"
    );
    assert!(matches!(
        window.await.unwrap(),
        PairOutcome::Stopped(ErrorCode::SasMismatch)
    ));
}

#[tokio::test]
async fn three_wrong_pairing_codes_close_the_window() {
    let (opener_doc, port, window) = open_pairing(true).await;
    for _ in 0..3 {
        let wrong = Secret::PairCode(Zeroizing::new("11111111".to_owned()));
        let joined = join(
            &target(port, &opener_doc, wrong),
            &alone(2),
            Some(JoinMode::Merge),
        )
        .await;
        assert!(matches!(joined, JoinPairOutcome::WrongCode), "{joined:?}");
    }

    assert!(matches!(
        window.await.unwrap(),
        PairOutcome::Closed(CloseReason::CodeLocked)
    ));
}

#[tokio::test]
async fn a_device_in_a_group_or_the_wrong_device_cannot_pair() {
    let (opener_doc, port, window) = open_pairing(true).await;
    let grouped = found_group(&alone(2), Platform::Linux, "Other", MINUTE_MS).unwrap();

    let in_group = join(
        &target(port, &opener_doc, Secret::PairSecret(SECRET)),
        &grouped,
        Some(JoinMode::Merge),
    )
    .await;
    let mut wrong_device = target(port, &opener_doc, Secret::PairSecret(SECRET));
    wrong_device.short_fingerprint = "0000".to_owned();
    let impostor = join(&wrong_device, &alone(2), Some(JoinMode::Merge)).await;

    assert!(
        matches!(in_group, JoinPairOutcome::InAGroup),
        "{in_group:?}"
    );
    assert!(
        matches!(impostor, JoinPairOutcome::Stopped(ErrorCode::NotInGroup)),
        "{impostor:?}"
    );
    window.abort();
}

#[tokio::test]
async fn a_paired_device_can_then_sync_with_the_device_that_added_it() {
    let (opener_doc, port, window) = open_pairing(true).await;
    let joined = join(
        &target(port, &opener_doc, Secret::PairSecret(SECRET)),
        &alone(2),
        Some(JoinMode::Merge),
    )
    .await;
    let JoinPairOutcome::Paired { result: phone, .. } = joined else {
        panic!("{joined:?}");
    };
    let PairOutcome::Paired { result: mac, .. } = window.await.unwrap() else {
        panic!("opener did not pair");
    };
    let (mac_doc, _) = add(&mac.doc, "Added later on the Mac", 3, 3 * MINUTE_MS);

    let mac_identity = identity_of(&mac_doc).unwrap();
    let phone_identity = identity_of(&phone.doc).unwrap();
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let sync_port = listeners.port();
    let sync_window = SyncWindow {
        me: mac_identity.device_id(),
        exchange: mac_identity.exchange_secret().clone(),
        hello: hello(
            mac_identity.device_id(),
            Some(mac_identity.dx_cert()),
            "Study Mac",
        ),
        code: Zeroizing::new(CODE.to_owned()),
        window_random: RANDOM,
        wait_ms: 60_000,
        timers: quick(),
    };
    let (events, mut asks) = mpsc::channel(4);
    tokio::spawn(async move {
        while let Some(event) = asks.recv().await {
            if let OpenerEvent::Request { answer, .. } = event {
                let _ = answer.send(true);
            }
        }
    });
    let current = Arc::new(move || mac_doc.clone());
    let opened = tokio::spawn(run_sync_window(
        sync_window,
        lan_acceptor(listeners),
        (),
        current,
        Arc::new(|| 4 * MINUTE_MS),
        events,
    ));
    let phone_joiner = Joiner {
        me: phone_identity.device_id(),
        exchange: phone_identity.exchange_secret().clone(),
        hello: hello(
            phone_identity.device_id(),
            Some(phone_identity.dx_cert()),
            "Phone",
        ),
        timers: quick(),
    };
    let found = FoundWindow {
        announcement: Announcement {
            kind: WindowKind::Sync,
            lan_name: "Study Mac".to_owned(),
            short_fingerprint: short_fingerprint(mac_identity.device_id()),
            remaining_s: 60,
            window_random: RANDOM,
            group_tag: Some([0; 6]),
        },
        route: crate::transport::Route::Lan {
            addrs: vec![(std::net::IpAddr::from([127, 0, 0, 1]), None)],
            port: sync_port,
        },
    };

    let synced = join_sync(
        &phone_joiner,
        &found,
        Zeroizing::new(CODE.to_owned()),
        &phone.doc,
        || 4 * MINUTE_MS,
    )
    .await;

    let JoinOutcome::Synced { result, .. } = synced else {
        panic!("{synced:?}");
    };
    assert_eq!(result.doc.entries.len(), 2);
    assert!(matches!(
        opened.await.unwrap(),
        WindowOutcome::Synced { .. }
    ));
}

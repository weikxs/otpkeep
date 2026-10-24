//! Whole sessions over TCP on loopback, with short timers.

use std::net::IpAddr;
use std::sync::Arc;

use keep_vault::{Platform, VaultDoc};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::listen::Listeners;
use super::sync_join::{JoinOutcome, Joiner, join_sync};
use super::sync_open::{OpenerEvent, SyncWindow, WindowOutcome, run_sync_window};
use crate::channel::PROTOCOL_VERSION;
use crate::channel::hello::Hello;
use crate::channel::message::ErrorCode;
use crate::identity::DeviceIdentity;
use crate::lan::announce::{Announcement, FoundWindow};
use crate::lan::interfaces::Eligible;
use crate::lan::window::{CloseReason, Timers, WindowKind};
use crate::testing::{MINUTE_MS, add, founded, identity, member_doc};

pub(super) const CODE: &str = "135790";
const RANDOM: [u8; 16] = [0x21; 16];

pub(super) fn quick() -> Timers {
    Timers {
        handshake_ms: 2_000,
        pair_confirm_ms: 2_000,
        sync_allow_ms: 600,
        data_idle_ms: 2_000,
        session_max_ms: 10_000,
        keepalive_ms: 100,
    }
}

pub(super) fn loopback() -> Eligible {
    Eligible {
        interface: "lo0".to_owned(),
        index: 1,
        addr: IpAddr::from([127, 0, 0, 1]),
        prefix: 8,
    }
}

fn hello(device: &DeviceIdentity, name: &str) -> Hello {
    Hello {
        device: device.device_id(),
        dx_cert: Some(device.dx_cert()),
        platform: Platform::Linux,
        lan_name: name.to_owned(),
        versions: vec![PROTOCOL_VERSION],
    }
}

fn joiner(n: u8) -> Joiner {
    let device = identity(n);
    Joiner {
        me: device.device_id(),
        exchange: device.exchange_secret().clone(),
        hello: hello(&device, "Phone"),
        timers: quick(),
    }
}

fn found(port: u16) -> FoundWindow {
    FoundWindow {
        announcement: Announcement {
            kind: WindowKind::Sync,
            lan_name: "Study Mac".to_owned(),
            short_fingerprint: "ABCD".to_owned(),
            remaining_s: 60,
            window_random: RANDOM,
            group_tag: Some([0; 6]),
        },
        addrs: vec![(IpAddr::from([127, 0, 0, 1]), None)],
        port,
    }
}

/// Opens a sync window on device 1 holding `doc`; the person answers every request with `allow`.
async fn open(
    doc: VaultDoc,
    allow: Option<bool>,
    wait_ms: u64,
) -> (u16, tokio::task::JoinHandle<WindowOutcome>) {
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let port = listeners.port();
    let device = identity(1);
    let window = SyncWindow {
        me: device.device_id(),
        exchange: device.exchange_secret().clone(),
        hello: hello(&device, "Study Mac"),
        code: Zeroizing::new(CODE.to_owned()),
        window_random: RANDOM,
        wait_ms,
        timers: quick(),
    };
    let (events, mut requests) = mpsc::channel(4);
    tokio::spawn(async move {
        // Without a decision, hold the answers: nobody responds.
        let mut unanswered = Vec::new();
        while let Some(event) = requests.recv().await {
            if let OpenerEvent::Request { answer, .. } = event {
                match allow {
                    Some(decision) => {
                        let _ = answer.send(decision);
                    }
                    None => unanswered.push(answer),
                }
            }
        }
    });
    let current = Arc::new(move || doc.clone());
    let task = tokio::spawn(run_sync_window(
        window,
        listeners,
        (),
        current,
        Arc::new(|| 5 * MINUTE_MS),
        events,
    ));
    (port, task)
}

fn group() -> Vec<keep_vault::MemberEvent> {
    founded(1, &[2, 3])
}

#[tokio::test]
async fn a_member_with_the_right_code_syncs_once_allowed() {
    let (a, _) = add(&member_doc(1, &group()), "On the Mac", 1, MINUTE_MS);
    let (b, _) = add(&member_doc(2, &group()), "On the phone", 2, MINUTE_MS);
    let (port, window) = open(a, Some(true), 60_000).await;

    let joined = join_sync(
        &joiner(2),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &b,
        || 5 * MINUTE_MS,
    )
    .await;
    let opened = window.await.unwrap();

    let JoinOutcome::Synced { result: on_b, peer } = joined else {
        panic!("joiner: {joined:?}");
    };
    let WindowOutcome::Synced { result: on_a, .. } = opened else {
        panic!("opener: {opened:?}");
    };
    assert_eq!(on_a.digest, on_b.digest);
    assert_eq!(on_b.doc.entries.len(), 2);
    assert_eq!(peer.device, identity(1).device_id());
}

#[tokio::test]
async fn a_declined_request_stops_both_sides() {
    let (port, window) = open(member_doc(1, &group()), Some(false), 60_000).await;

    let joined = join_sync(
        &joiner(2),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(2, &group()),
        || 0,
    )
    .await;

    assert!(
        matches!(joined, JoinOutcome::Stopped(ErrorCode::Rejected)),
        "{joined:?}"
    );
    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Stopped(ErrorCode::Rejected)
    ));
}

#[tokio::test]
async fn an_unanswered_request_times_out() {
    let (port, window) = open(member_doc(1, &group()), None, 60_000).await;

    let joined = join_sync(
        &joiner(2),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(2, &group()),
        || 0,
    )
    .await;

    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Closed(CloseReason::AllowTimeout)
    ));
    assert!(
        matches!(joined, JoinOutcome::Lost | JoinOutcome::Stopped(_)),
        "{joined:?}"
    );
}

#[tokio::test]
async fn three_wrong_codes_close_the_window() {
    let (port, window) = open(member_doc(1, &group()), Some(true), 60_000).await;
    let b = member_doc(2, &group());

    for _ in 0..3 {
        let joined = join_sync(
            &joiner(2),
            &found(port),
            Zeroizing::new("000000".to_owned()),
            &b,
            || 0,
        )
        .await;
        assert!(matches!(joined, JoinOutcome::WrongCode), "{joined:?}");
    }

    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Closed(CloseReason::CodeLocked)
    ));
    let after = join_sync(
        &joiner(2),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &b,
        || 0,
    )
    .await;
    assert!(
        matches!(after, JoinOutcome::Unreachable | JoinOutcome::Lost),
        "{after:?}"
    );
}

#[tokio::test]
async fn a_second_device_is_told_the_window_is_busy_during_a_handshake() {
    let (port, window) = open(member_doc(1, &group()), None, 60_000).await;
    // A connection that never finishes its handshake occupies the window.
    let _stalled = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let second = join_sync(
        &joiner(3),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(3, &group()),
        || 0,
    )
    .await;

    assert!(matches!(second, JoinOutcome::Busy), "{second:?}");
    window.abort();
}

#[tokio::test]
async fn once_a_code_is_verified_the_window_stops_listening() {
    let (port, window) = open(member_doc(1, &group()), None, 60_000).await;
    let first = tokio::spawn(async move {
        join_sync(
            &joiner(2),
            &found(port),
            Zeroizing::new(CODE.to_owned()),
            &member_doc(2, &group()),
            || 0,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let second = join_sync(
        &joiner(3),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(3, &group()),
        || 0,
    )
    .await;

    assert!(matches!(second, JoinOutcome::Unreachable), "{second:?}");
    let _ = first.await;
    let _ = window.await;
}

#[tokio::test]
async fn a_device_outside_the_group_is_refused_without_using_up_a_code_attempt() {
    let (port, window) = open(member_doc(1, &group()), Some(true), 60_000).await;
    let stranger_doc = member_doc(9, &founded(9, &[]));

    let refused = join_sync(
        &joiner(9),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &stranger_doc,
        || 0,
    )
    .await;
    let member = join_sync(
        &joiner(2),
        &found(port),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(2, &group()),
        || 0,
    )
    .await;

    assert!(
        matches!(refused, JoinOutcome::Stopped(ErrorCode::NotInGroup)),
        "{refused:?}"
    );
    assert!(matches!(member, JoinOutcome::Synced { .. }), "{member:?}");
    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Synced { .. }
    ));
}

#[tokio::test]
async fn an_unused_window_expires() {
    let (_, window) = open(member_doc(1, &group()), Some(true), 300).await;

    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Closed(CloseReason::Expired)
    ));
}

mod pair;

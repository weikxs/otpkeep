//! One window across transports: a real local-network listener and a fake direct transport.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::{CODE, RANDOM, group, hello, joiner, lan_acceptor, loopback, quick};
use crate::flow::listen::Listeners;
use crate::flow::sync_join::{JoinOutcome, join_sync};
use crate::flow::sync_open::{OpenerEvent, SyncWindow, WindowOutcome, run_sync_window};
use crate::lan::announce::{Announcement, FoundWindow};
use crate::lan::window::WindowKind;
use crate::testing::{MINUTE_MS, add, identity, member_doc};
use crate::transport::tests::FakeRadio;
use crate::transport::{Route, Via};

/// Opens device 1's sync window on loopback and on a fake radio; requests are allowed when `allow`.
async fn open_both(allow: Option<bool>) -> (Route, Route, tokio::task::JoinHandle<WindowOutcome>) {
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let port = listeners.port();
    let acceptor = lan_acceptor(listeners);
    let radio = Route::Direct {
        via: Via::Bluetooth,
        connector: Arc::new(FakeRadio(acceptor.sender())),
    };
    let lan = Route::Lan {
        addrs: vec![(std::net::IpAddr::from([127, 0, 0, 1]), None)],
        port,
    };
    let device = identity(1);
    let window = SyncWindow {
        me: device.device_id(),
        exchange: device.exchange_secret().clone(),
        hello: hello(&device, "Study Mac"),
        code: Zeroizing::new(CODE.to_owned()),
        window_random: RANDOM,
        wait_ms: 60_000,
        timers: quick(),
    };
    let (events, mut asks) = mpsc::channel(4);
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Some(event) = asks.recv().await {
            if let OpenerEvent::Request { answer, .. } = event {
                match allow {
                    Some(decision) => {
                        let _ = answer.send(decision);
                    }
                    None => held.push(answer),
                }
            }
        }
    });
    let (doc, _) = add(&member_doc(1, &group()), "On the Mac", 1, MINUTE_MS);
    let current = Arc::new(move || doc.clone());
    let task = tokio::spawn(run_sync_window(
        window,
        acceptor,
        (),
        current,
        Arc::new(|| 5 * MINUTE_MS),
        events,
    ));
    (lan, radio, task)
}

fn found(route: Route) -> FoundWindow {
    FoundWindow {
        announcement: Announcement {
            kind: WindowKind::Sync,
            lan_name: "Study Mac".to_owned(),
            short_fingerprint: "ABCD".to_owned(),
            remaining_s: 60,
            window_random: RANDOM,
            group_tag: Some([0; 6]),
        },
        route,
    }
}

#[tokio::test]
async fn a_sync_runs_the_same_over_a_direct_transport() {
    let (_, radio, window) = open_both(Some(true)).await;

    let joined = join_sync(
        &joiner(2),
        &found(radio),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(2, &group()),
        || 5 * MINUTE_MS,
    )
    .await;

    let JoinOutcome::Synced { result, .. } = joined else {
        panic!("{joined:?}");
    };
    assert_eq!(result.doc.entries.len(), 1);
    assert!(matches!(
        window.await.unwrap(),
        WindowOutcome::Synced { .. }
    ));
}

#[tokio::test]
async fn one_connection_at_a_time_across_transports() {
    let (lan, radio, window) = open_both(None).await;
    // A local-network connection that never finishes its handshake occupies the window.
    let _stalled = lan.connect(Duration::from_secs(1)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    let joined = join_sync(
        &joiner(3),
        &found(radio),
        Zeroizing::new(CODE.to_owned()),
        &member_doc(3, &group()),
        || 0,
    )
    .await;

    assert!(matches!(joined, JoinOutcome::Busy), "{joined:?}");
    window.abort();
}

#[tokio::test]
async fn a_code_verified_on_one_transport_closes_the_others() {
    let (lan, radio, window) = open_both(None).await;
    let first = tokio::spawn(async move {
        join_sync(
            &joiner(2),
            &found(radio),
            Zeroizing::new(CODE.to_owned()),
            &member_doc(2, &group()),
            || 0,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        lan.connect(Duration::from_millis(300)).await.is_none(),
        "the local network stopped listening"
    );
    window.abort();
    first.abort();
}

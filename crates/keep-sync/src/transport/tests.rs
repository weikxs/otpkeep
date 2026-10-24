use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};
use tokio::sync::mpsc;

use super::{Acceptor, BoxLink, Connecting, Connector, Incoming, Route, Via};
use crate::flow::listen::Listeners;
use crate::lan::interfaces::Eligible;

/// Whether `route` stops accepting connections within `within`. Closing a listener is asynchronous (its accept
/// task is aborted and the socket closes when the runtime next runs it, later on Windows), so tests wait for the
/// refusal instead of expecting it at once or after a fixed sleep.
pub(crate) async fn refused_within(route: &Route, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if route.connect(Duration::from_millis(200)).await.is_none() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A stand-in for a direct transport: each connection is an in-memory pipe handed to the window.
pub(crate) struct FakeRadio(pub(crate) mpsc::Sender<Incoming>);

impl Connector for FakeRadio {
    fn connect(&self) -> Connecting<'_> {
        Box::pin(async move {
            let (ours, theirs) = duplex(1 << 16);
            let link: BoxLink = Box::new(theirs);
            self.0
                .send(Incoming {
                    link,
                    via: Via::Bluetooth,
                })
                .await
                .map_err(|_| std::io::Error::other("the window is gone"))?;
            Ok(Box::new(ours) as BoxLink)
        })
    }
}

fn loopback() -> Eligible {
    Eligible {
        interface: "lo0".to_owned(),
        index: 1,
        addr: std::net::IpAddr::from([127, 0, 0, 1]),
        prefix: 8,
    }
}

async fn echo_once(acceptor: &mut Acceptor) -> Via {
    let mut incoming = acceptor.next().await.unwrap();
    let mut byte = [0u8; 1];
    incoming.link.read_exact(&mut byte).await.unwrap();
    incoming.link.write_all(&byte).await.unwrap();
    incoming.via
}

#[tokio::test]
async fn one_acceptor_takes_connections_from_every_transport() {
    let mut acceptor = Acceptor::new();
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let port = listeners.port();
    acceptor.add_lan(listeners);
    let radio = Route::Direct {
        via: Via::Bluetooth,
        connector: Arc::new(FakeRadio(acceptor.sender())),
    };
    let lan = Route::Lan {
        addrs: vec![(std::net::IpAddr::from([127, 0, 0, 1]), None)],
        port,
    };

    for route in [&lan, &radio] {
        let mut link = route.connect(Duration::from_secs(2)).await.unwrap();
        link.write_all(&[7]).await.unwrap();
        assert_eq!(echo_once(&mut acceptor).await, route.via());
        let mut back = [0u8; 1];
        link.read_exact(&mut back).await.unwrap();
        assert_eq!(back, [7]);
    }
}

#[tokio::test]
async fn nothing_can_connect_once_the_acceptor_is_gone() {
    let acceptor = Acceptor::new();
    let radio = Route::Direct {
        via: Via::Bluetooth,
        connector: Arc::new(FakeRadio(acceptor.sender())),
    };
    let listeners = Listeners::bind(&[loopback()]).await.unwrap();
    let lan = Route::Lan {
        addrs: vec![(std::net::IpAddr::from([127, 0, 0, 1]), None)],
        port: listeners.port(),
    };
    drop(listeners);
    drop(acceptor);

    assert!(radio.connect(Duration::from_millis(200)).await.is_none());
    assert!(
        refused_within(&lan, Duration::from_secs(5)).await,
        "the local network kept accepting"
    );
}

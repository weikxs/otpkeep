//! Transports: where a window is announced and found, and the byte stream a session runs on
//! (design/06-sync.md §3.6, ADR-017).
//!
//! Sessions need nothing from a transport but a byte stream: all security is in the protocol. A window takes
//! connections from every transport through one [`Acceptor`], so one-to-one, the wrong-code count and the
//! timers hold across transports. A found window carries a [`Route`] that says how to reach it.

use std::fmt;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr, SocketAddrV6};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::flow::listen::Listeners;

/// A byte stream from any transport.
pub trait Link: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Link for T {}

/// A boxed byte stream; sessions run on this.
pub type BoxLink = Box<dyn Link>;

/// Which transport a connection came over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Via {
    /// The local network.
    Lan,
    /// Bluetooth Low Energy.
    Bluetooth,
}

/// A connection a window received.
pub struct Incoming {
    /// The byte stream.
    pub link: BoxLink,
    /// Its transport.
    pub via: Via,
}

const QUEUE: usize = 16;

/// The connections of one window, from all of its transports. Dropping it stops every transport it holds.
pub struct Acceptor {
    sender: mpsc::Sender<Incoming>,
    incoming: mpsc::Receiver<Incoming>,
    tasks: Vec<JoinHandle<()>>,
    guards: Vec<Box<dyn Send>>,
}

impl Default for Acceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl Acceptor {
    /// An acceptor with no transports yet.
    #[must_use]
    pub fn new() -> Self {
        let (sender, incoming) = mpsc::channel(QUEUE);
        Self {
            sender,
            incoming,
            tasks: Vec::new(),
            guards: Vec::new(),
        }
    }

    /// Takes the local network's connections.
    pub fn add_lan(&mut self, mut listeners: Listeners) {
        let sender = self.sender.clone();
        self.tasks.push(tokio::spawn(async move {
            while let Some(incoming) = listeners.next().await {
                let link: BoxLink = Box::new(incoming.stream);
                if sender
                    .send(Incoming {
                        link,
                        via: Via::Lan,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    /// Where another transport delivers its connections.
    #[must_use]
    pub fn sender(&self) -> mpsc::Sender<Incoming> {
        self.sender.clone()
    }

    /// Keeps `guard` (such as a transport's announcement or listener) alive until the acceptor is dropped.
    pub fn hold(&mut self, guard: impl Send + 'static) {
        self.guards.push(Box::new(guard));
    }

    /// The next connection from any transport.
    pub async fn next(&mut self) -> Option<Incoming> {
        self.incoming.recv().await
    }
}

impl Drop for Acceptor {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// The future a [`Connector`] returns.
pub type Connecting<'a> = Pin<Box<dyn Future<Output = io::Result<BoxLink>> + Send + 'a>>;

/// Opens a byte stream to a window found on a transport without IP addresses, such as Bluetooth.
pub trait Connector: Send + Sync {
    /// Connects.
    fn connect(&self) -> Connecting<'_>;
}

/// How to reach a found window.
#[derive(Clone)]
pub enum Route {
    /// The local network: addresses on this device's links, with IPv6 scopes, and the port.
    Lan {
        /// Addresses.
        addrs: Vec<(IpAddr, Option<u32>)>,
        /// Port.
        port: u16,
    },
    /// A transport that connects directly.
    Direct {
        /// Which transport.
        via: Via,
        /// Opens the stream.
        connector: Arc<dyn Connector>,
    },
}

impl Route {
    /// The transport.
    #[must_use]
    pub fn via(&self) -> Via {
        match self {
            Self::Lan { .. } => Via::Lan,
            Self::Direct { via, .. } => *via,
        }
    }

    /// Opens a byte stream within `limit`; `None` when nothing answers.
    pub async fn connect(&self, limit: Duration) -> Option<BoxLink> {
        match self {
            Self::Lan { addrs, port } => {
                for (addr, scope) in addrs {
                    let socket = match (addr, scope) {
                        (IpAddr::V6(v6), Some(scope)) => {
                            SocketAddr::V6(SocketAddrV6::new(*v6, *port, 0, *scope))
                        }
                        _ => SocketAddr::new(*addr, *port),
                    };
                    if let Ok(Ok(stream)) =
                        tokio::time::timeout(limit, TcpStream::connect(socket)).await
                    {
                        return Some(Box::new(stream));
                    }
                }
                None
            }
            Self::Direct { connector, .. } => tokio::time::timeout(limit, connector.connect())
                .await
                .ok()
                .and_then(Result::ok),
        }
    }
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lan { addrs, port } => f
                .debug_struct("Lan")
                .field("addrs", addrs)
                .field("port", port)
                .finish(),
            Self::Direct { via, .. } => f
                .debug_struct("Direct")
                .field("via", via)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;

//! Listening on the eligible addresses only, with one port for all, and dropping peers from other links.

use std::net::{IpAddr, SocketAddr, SocketAddrV6};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::lan::interfaces::{Eligible, peer_is_on_link};

const QUEUE: usize = 16;

/// Why listening failed.
#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    /// No address may be used (`NO_ELIGIBLE_INTERFACE`).
    #[error("there is no usable local network connection")]
    NoInterface,
    /// The system refused.
    #[error("the window could not start listening")]
    Io(#[from] std::io::Error),
}

/// A connection from a peer on the link of the address it reached.
pub struct Incoming {
    /// The connection.
    pub stream: TcpStream,
    /// The peer's address.
    pub peer: SocketAddr,
}

/// Listening sockets; they close when dropped.
pub struct Listeners {
    port: u16,
    incoming: mpsc::Receiver<Incoming>,
    tasks: Vec<JoinHandle<()>>,
}

impl Listeners {
    /// Listens on every eligible address with the same random port (on at least the first address).
    ///
    /// # Errors
    ///
    /// Returns [`ListenError`].
    pub async fn bind(addrs: &[Eligible]) -> Result<Self, ListenError> {
        let first = addrs.first().ok_or(ListenError::NoInterface)?;
        let listener = TcpListener::bind(socket(first, 0)).await?;
        let port = listener.local_addr()?.port();
        let mut bound = vec![(listener, first.clone())];
        for addr in &addrs[1..] {
            match TcpListener::bind(socket(addr, port)).await {
                Ok(listener) => bound.push((listener, addr.clone())),
                Err(error) => tracing::warn!(%error, "could not listen on one address"),
            }
        }
        let (sender, incoming) = mpsc::channel(QUEUE);
        let tasks = bound
            .into_iter()
            .map(|(listener, local)| tokio::spawn(accept_loop(listener, local, sender.clone())))
            .collect();
        Ok(Self {
            port,
            incoming,
            tasks,
        })
    }

    /// The port every address listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The next connection from a peer on the right link.
    pub async fn next(&mut self) -> Option<Incoming> {
        self.incoming.recv().await
    }
}

impl Drop for Listeners {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

fn socket(addr: &Eligible, port: u16) -> SocketAddr {
    match addr.addr {
        IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80 => {
            SocketAddr::V6(SocketAddrV6::new(v6, port, 0, addr.index))
        }
        ip => SocketAddr::new(ip, port),
    }
}

async fn accept_loop(listener: TcpListener, local: Eligible, sender: mpsc::Sender<Incoming>) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            continue;
        };
        let scope = match peer {
            SocketAddr::V6(v6) => Some(v6.scope_id()),
            SocketAddr::V4(_) => None,
        };
        if !peer_is_on_link(&local, peer.ip(), scope) {
            tracing::warn!("refused a connection from another network (NOT_SAME_NETWORK)");
            continue;
        }
        if sender.send(Incoming { stream, peer }).await.is_err() {
            return;
        }
    }
}

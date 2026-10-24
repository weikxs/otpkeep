//! The opener's side of a sync window (design/06-sync.md §5.3–5.4).
//!
//! The window runs on a monotonic clock. One connection is handled at a time in its own task; others get
//! `BUSY`. A connection goes through the handshake, the member check, the sync code and the person's "allow",
//! then the data phase. When the code is right the window stops listening and announcing; when a timer runs
//! out or the person closes the window, the connection is dropped and nothing is written.

use std::sync::Arc;
use std::time::Duration;

use keep_vault::{DeviceId, VaultDoc};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval, timeout};
use x25519_dalek::StaticSecret;
use zeroize::Zeroizing;

use super::listen::{Incoming, Listeners};
use super::peer::{PeerInfo, check_member};
use crate::channel::code::{CodeCheck, CodeError, Secret, confirm_code};
use crate::channel::hello::Hello;
use crate::channel::message::{ErrorCode, Message};
use crate::channel::noise::{Channel, Purpose, accept_noise, refuse_busy};
use crate::lan::window::{
    Accept, CloseReason, ConnectionId, LanWindow, Timers, WindowKind, WindowStatus,
};
use crate::session::exchange::{Exchanged, SessionError, exchange_as_opener};

const TICK: Duration = Duration::from_millis(100);
/// How long a refused connection may take to hear `BUSY`.
const BUSY_REPLY: Duration = Duration::from_secs(2);

/// Reads the vault as it is now; called when a connection needs it.
pub type CurrentDoc = Arc<dyn Fn() -> VaultDoc + Send + Sync>;
/// The wall clock in Unix milliseconds.
pub type WallClock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// What the opener's person is asked or told.
#[derive(Debug)]
pub enum OpenerEvent {
    /// A member entered the right code: allow (`true`) or decline. No answer within `SYNC_ALLOW` declines.
    Request {
        /// Who asks.
        peer: PeerInfo,
        /// The answer.
        answer: oneshot::Sender<bool>,
    },
    /// Data is being exchanged.
    Exchanging {
        /// With whom.
        peer: PeerInfo,
    },
}

/// How a sync window ended.
#[derive(Debug)]
pub enum WindowOutcome {
    /// Synced: the caller commits `result.doc`.
    Synced {
        /// The other device.
        peer: PeerInfo,
        /// What to commit.
        result: Box<Exchanged>,
    },
    /// A device turned out to be removed: commit `keep`, which only adds membership events.
    Removed {
        /// What to commit.
        keep: Box<VaultDoc>,
    },
    /// The window closed without a sync.
    Closed(CloseReason),
    /// The session stopped with this code.
    Stopped(ErrorCode),
}

/// What the window needs.
pub struct SyncWindow {
    /// This device.
    pub me: DeviceId,
    /// Its X25519 secret.
    pub exchange: StaticSecret,
    /// Its introduction.
    pub hello: Hello,
    /// The code shown on screen.
    pub code: Zeroizing<String>,
    /// The window's random value, also announced.
    pub window_random: [u8; 16],
    /// `WINDOW_WAIT`.
    pub wait_ms: u64,
    /// The session timers.
    pub timers: Timers,
}

/// What a connection's task reports.
enum Progress {
    CodePassed,
    CodeFailed,
    Dropped,
    Agreed,
    Finished(WindowOutcome),
}

/// Runs a sync window until it closes. `announcing` is dropped as soon as no more connections are taken.
pub async fn run_sync_window<A: Send>(
    window: SyncWindow,
    listeners: Listeners,
    announcing: A,
    current: CurrentDoc,
    wall: WallClock,
    events: mpsc::Sender<OpenerEvent>,
) -> WindowOutcome {
    let started = Instant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut state = LanWindow::with_timers(WindowKind::Sync, window.wait_ms, now(), window.timers);
    let shared = Arc::new(window);
    let mut listeners = Some(listeners);
    let mut announcing = Some(announcing);
    let mut current_task: Option<(
        ConnectionId,
        tokio::task::JoinHandle<()>,
        mpsc::Receiver<Progress>,
    )> = None;
    let mut ticker = interval(TICK);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Some(reason) = state.tick(now()) {
                    abort(&mut current_task);
                    return WindowOutcome::Closed(reason);
                }
                if state.status() == WindowStatus::Waiting && current_task.is_some() {
                    abort(&mut current_task);
                }
            }
            incoming = next_incoming(&mut listeners) => {
                let Some(Incoming { stream, .. }) = incoming else { continue };
                match state.accept(now()) {
                    Accept::Handle(id) => {
                        let (progress, reports) = mpsc::channel(8);
                        let task = tokio::spawn(serve(
                            stream, Arc::clone(&shared), Arc::clone(&current), Arc::clone(&wall), events.clone(), progress,
                        ));
                        current_task = Some((id, task, reports));
                    }
                    Accept::Busy => {
                        tokio::spawn(async move { let _ = timeout(BUSY_REPLY, refuse_busy(stream)).await; });
                    }
                    Accept::Closed => {
                        abort(&mut current_task);
                        return WindowOutcome::Closed(closed_reason(&state));
                    }
                }
            }
            report = next_report(&mut current_task) => {
                let Some((id, progress)) = report else { continue };
                match progress {
                    Progress::CodePassed => {
                        state.code_passed(id, now());
                        listeners = None;
                        announcing = None;
                    }
                    Progress::CodeFailed => {
                        current_task = None;
                        if let Some(reason) = state.code_failed(id, now()) {
                            return WindowOutcome::Closed(reason);
                        }
                    }
                    Progress::Dropped => {
                        current_task = None;
                        state.dropped(id, now());
                    }
                    Progress::Agreed => state.person_agreed(now()),
                    Progress::Finished(outcome) => {
                        state.finish(now());
                        drop(announcing);
                        return outcome;
                    }
                }
            }
        }
    }
}

fn closed_reason(state: &LanWindow) -> CloseReason {
    match state.status() {
        WindowStatus::Closed(reason) => reason,
        _ => CloseReason::Cancelled,
    }
}

fn abort(
    task: &mut Option<(
        ConnectionId,
        tokio::task::JoinHandle<()>,
        mpsc::Receiver<Progress>,
    )>,
) {
    if let Some((_, handle, _)) = task.take() {
        handle.abort();
    }
}

async fn next_incoming(listeners: &mut Option<Listeners>) -> Option<Incoming> {
    match listeners {
        Some(listeners) => listeners.next().await,
        None => std::future::pending().await,
    }
}

async fn next_report(
    task: &mut Option<(
        ConnectionId,
        tokio::task::JoinHandle<()>,
        mpsc::Receiver<Progress>,
    )>,
) -> Option<(ConnectionId, Progress)> {
    match task {
        Some((id, _, reports)) => {
            let id = *id;
            reports.recv().await.map(|p| (id, p))
        }
        None => std::future::pending().await,
    }
}

/// One connection, from the handshake to the end of the data phase.
async fn serve(
    stream: TcpStream,
    window: Arc<SyncWindow>,
    current: CurrentDoc,
    wall: WallClock,
    events: mpsc::Sender<OpenerEvent>,
    progress: mpsc::Sender<Progress>,
) {
    let report = |p: Progress| {
        let progress = progress.clone();
        async move {
            let _ = progress.send(p).await;
        }
    };
    let Ok((mut channel, hello)) = accept_noise(
        stream,
        Purpose::Sync,
        &window.window_random,
        &window.exchange,
        &window.hello,
    )
    .await
    else {
        return report(Progress::Dropped).await;
    };
    let peer = match check_member(&current(), &hello, channel.remote_static()) {
        Ok(peer) => peer,
        Err(code) => {
            let _ = channel.send(&Message::Error(code).encode()).await;
            return report(Progress::Dropped).await;
        }
    };
    let secret = Secret::SyncCode(window.code.clone());
    match confirm_code(&mut channel, CodeCheck::Opener(&secret)).await {
        Ok(_) => report(Progress::CodePassed).await,
        Err(CodeError::Wrong) => return report(Progress::CodeFailed).await,
        Err(_) => return report(Progress::Dropped).await,
    }
    if !ask_person(&mut channel, &peer, &events, window.timers.keepalive_ms).await {
        let _ = channel
            .send(&Message::Error(ErrorCode::Rejected).encode())
            .await;
        return report(Progress::Finished(WindowOutcome::Stopped(
            ErrorCode::Rejected,
        )))
        .await;
    }
    if channel.send(&Message::Agreed.encode()).await.is_err() {
        return report(Progress::Finished(WindowOutcome::Stopped(
            ErrorCode::Cancelled,
        )))
        .await;
    }
    report(Progress::Agreed).await;
    let _ = events
        .send(OpenerEvent::Exchanging { peer: peer.clone() })
        .await;
    channel.set_idle_timeout(Some(Duration::from_millis(window.timers.data_idle_ms)));
    let outcome =
        match exchange_as_opener(&mut channel, &current(), window.me, peer.device, wall()).await {
            Ok(result) => WindowOutcome::Synced {
                peer,
                result: Box::new(result),
            },
            Err(SessionError::Removed { keep }) => WindowOutcome::Removed { keep },
            Err(SessionError::Stopped(code)) => WindowOutcome::Stopped(code),
            Err(SessionError::Lost) => WindowOutcome::Stopped(ErrorCode::Cancelled),
        };
    report(Progress::Finished(outcome)).await;
}

/// Asks the person, sending keepalives meanwhile. The window's timer ends the wait.
async fn ask_person(
    channel: &mut Channel<TcpStream>,
    peer: &PeerInfo,
    events: &mpsc::Sender<OpenerEvent>,
    keepalive_ms: u64,
) -> bool {
    let (answer, mut decision) = oneshot::channel();
    if events
        .send(OpenerEvent::Request {
            peer: peer.clone(),
            answer,
        })
        .await
        .is_err()
    {
        return false;
    }
    let mut keepalive = interval(Duration::from_millis(keepalive_ms));
    keepalive.tick().await;
    loop {
        tokio::select! {
            decided = &mut decision => return decided.unwrap_or(false),
            _ = keepalive.tick() => {
                if channel.send(&Message::Keepalive.encode()).await.is_err() {
                    return false;
                }
            }
        }
    }
}

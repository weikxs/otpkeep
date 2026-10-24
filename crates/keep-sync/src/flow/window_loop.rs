//! The opener's window loop, shared by pairing and sync: one connection at a time in its own task, `BUSY` for
//! the rest, the window's timers, and no more listening or announcing once a code is verified.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval, timeout};

use crate::channel::noise::refuse_busy;
use crate::lan::window::{
    Accept, CloseReason, ConnectionId, LanWindow, Timers, WindowKind, WindowStatus,
};
use crate::transport::{Acceptor, BoxLink, Incoming};

const TICK: Duration = Duration::from_millis(100);
/// How long a refused connection may take to hear `BUSY`.
const BUSY_REPLY: Duration = Duration::from_secs(2);

/// What a connection's task reports to the loop.
pub(crate) enum Progress<O> {
    /// The code was right.
    CodePassed,
    /// The code was wrong; the window counts it.
    CodeFailed,
    /// The connection ended before its code was verified, for another reason.
    Dropped,
    /// The people agreed; data flows.
    Agreed,
    /// The session ended.
    Finished(O),
}

type Current<O> = Option<(ConnectionId, JoinHandle<()>, mpsc::Receiver<Progress<O>>)>;

/// Runs a window until a session finishes (`Ok`) or the window closes (`Err`).
pub(crate) async fn run_window<O: Send + 'static, A>(
    kind: WindowKind,
    wait_ms: u64,
    timers: Timers,
    acceptor: Acceptor,
    announcing: A,
    serve: impl Fn(BoxLink, mpsc::Sender<Progress<O>>) -> JoinHandle<()>,
) -> Result<O, CloseReason> {
    let started = Instant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut state = LanWindow::with_timers(kind, wait_ms, now(), timers);
    let mut acceptor = Some(acceptor);
    let mut announcing = Some(announcing);
    let mut current: Current<O> = None;
    let mut ticker = interval(TICK);
    loop {
        tokio::select! {
            // Reports first: a connection that just ended must free the window before the next one is judged.
            biased;
            report = next_report(&mut current) => {
                let Some((id, progress)) = report else {
                    // The task ended without a report: treat it as dropped.
                    if let Some((id, _, _)) = current.take() {
                        state.dropped(id, now());
                    }
                    continue;
                };
                match progress {
                    Progress::CodePassed => {
                        state.code_passed(id, now());
                        acceptor = None;
                        announcing = None;
                    }
                    Progress::CodeFailed => {
                        current = None;
                        if let Some(reason) = state.code_failed(id, now()) {
                            return Err(reason);
                        }
                    }
                    Progress::Dropped => {
                        current = None;
                        state.dropped(id, now());
                    }
                    Progress::Agreed => state.person_agreed(now()),
                    Progress::Finished(outcome) => {
                        state.finish(now());
                        drop(announcing);
                        return Ok(outcome);
                    }
                }
            }
            _ = ticker.tick() => {
                if let Some(reason) = state.tick(now()) {
                    abort(&mut current);
                    return Err(reason);
                }
                if state.status() == WindowStatus::Waiting && current.is_some() {
                    abort(&mut current);
                }
            }
            incoming = next_incoming(&mut acceptor) => {
                let Some(Incoming { link, .. }) = incoming else { continue };
                match state.accept(now()) {
                    Accept::Handle(id) => {
                        let (progress, reports) = mpsc::channel(8);
                        current = Some((id, serve(link, progress), reports));
                    }
                    Accept::Busy => {
                        tokio::spawn(async move {
                            let _ = timeout(BUSY_REPLY, refuse_busy(link)).await;
                        });
                    }
                    Accept::Closed => {
                        abort(&mut current);
                        return Err(match state.status() {
                            WindowStatus::Closed(reason) => reason,
                            _ => CloseReason::Cancelled,
                        });
                    }
                }
            }
        }
    }
}

fn abort<O>(current: &mut Current<O>) {
    if let Some((_, handle, _)) = current.take() {
        handle.abort();
    }
}

async fn next_incoming(acceptor: &mut Option<Acceptor>) -> Option<Incoming> {
    match acceptor {
        Some(acceptor) => acceptor.next().await,
        None => std::future::pending().await,
    }
}

/// The next report; `None` when the task ended without one.
async fn next_report<O>(current: &mut Current<O>) -> Option<(ConnectionId, Progress<O>)> {
    match current {
        Some((id, _, reports)) => {
            let id = *id;
            reports.recv().await.map(|p| (id, p))
        }
        None => std::future::pending().await,
    }
}

//! Pairing and sync sessions for the app (design/06-sync.md §4, §5, §8). One session at a time; questions
//! for the person (allow a request, compare the check code) wait here until the page answers them, while
//! the session keeps running. Every change is reported through [`Notify`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use keep_link::Network;
use keep_sync::flow::peer::PeerInfo;
use keep_sync::lan::announce::FoundWindow;
use keep_vault::history::HistoryField;
use keep_vault::{DeviceId, EntryId, VaultDoc};
use tokio::sync::oneshot;

use crate::core::{Clock, Core, CoreError};

pub mod join;
mod view;
mod window;

pub use join::FoundView;
pub use view::{AskKind, Phase, ResultView, SessionKind, SessionView, SyncView};

/// The vault, shared by the window's commands and the sessions.
pub type SharedCore = Arc<Mutex<Core>>;

/// `SYNC_ALLOW`: how long a request waits for "allow".
const ALLOW_MS: u64 = 30 * 1000;
/// `PAIR_CONFIRM`: how long the check code waits.
const CONFIRM_MS: u64 = 60 * 1000;

/// Where the sessions report to: the page, and the windows that show the data.
pub trait Notify: Send + Sync + 'static {
    /// The session or the last result changed.
    fn view(&self, view: &SyncView);
    /// A session saved new data.
    fn vault_changed(&self);
}

/// Why a session could not start; the page words each code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SyncError(pub &'static str);

impl From<CoreError> for SyncError {
    fn from(error: CoreError) -> Self {
        Self(match error {
            CoreError::NeedsPassword => "NEEDS_PASSWORD",
            CoreError::ReadOnly => "READ_ONLY",
            CoreError::Invalid(rule) => rule,
            CoreError::NotFound => "NOT_FOUND",
            _ => "SAVE_FAILED",
        })
    }
}

impl From<keep_link::LinkError> for SyncError {
    fn from(error: keep_link::LinkError) -> Self {
        Self(error.code)
    }
}

/// How a session ended.
enum Ended {
    /// It produced data to save: synced, paired, or (when this device was removed) the group events only.
    Save {
        code: &'static str,
        doc: Box<VaultDoc>,
        overwritten: Vec<(EntryId, HistoryField, String)>,
        participants: [DeviceId; 2],
    },
    /// Nothing to save.
    Stop(&'static str),
}

/// A session's result to save.
fn saved(
    code: &'static str,
    doc: VaultDoc,
    overwritten: Vec<(EntryId, HistoryField, String)>,
    participants: [DeviceId; 2],
) -> Ended {
    Ended::Save {
        code,
        doc: Box::new(doc),
        overwritten,
        participants,
    }
}

/// Runs this device's pairing and sync sessions.
#[derive(Clone)]
pub struct Syncer {
    inner: Arc<Inner>,
}

struct Inner {
    core: SharedCore,
    network: Network,
    notify: Arc<dyn Notify>,
    clock: Clock,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    view: SyncView,
    asks: HashMap<u64, oneshot::Sender<bool>>,
    next_ask: u64,
    found: Vec<FoundWindow>,
    /// The open pairing window's invite QR code (SVG), kept back until asked for.
    invite: Option<String>,
    task: Option<tokio::task::AbortHandle>,
    /// Counts sessions, so a session that was closed cannot report afterwards.
    generation: u64,
}

impl Syncer {
    #[must_use]
    pub fn new(core: SharedCore, network: Network, notify: Arc<dyn Notify>, clock: Clock) -> Self {
        Self {
            inner: Arc::new(Inner {
                core,
                network,
                notify,
                clock,
                state: Mutex::new(State::default()),
            }),
        }
    }

    /// The session in progress and the last result.
    #[must_use]
    pub fn view(&self) -> SyncView {
        self.state().view.clone()
    }

    /// Answers a question the page shows: allow / decline a request, or same / different check code.
    ///
    /// # Errors
    ///
    /// Returns `ASK_GONE` when the question timed out or was answered already.
    pub fn answer(&self, id: u64, yes: bool) -> Result<(), SyncError> {
        let sender = {
            let mut state = self.state();
            let sender = state.asks.remove(&id).ok_or(SyncError("ASK_GONE"))?;
            if let Some(session) = &mut state.view.session
                && session.ask.as_ref().is_some_and(|a| a.id == id)
            {
                session.ask = None;
                session.phase = view::Phase::Connecting;
            }
            sender
        };
        let _ = sender.send(yes);
        self.publish();
        Ok(())
    }

    /// The open pairing window's invite QR code; the caller has the person confirm first.
    #[must_use]
    pub fn invite(&self) -> Option<String> {
        let state = self.state();
        state.view.session.as_ref()?;
        state.invite.clone()
    }

    /// Closes the window or stops the session; nothing is saved from it.
    pub fn close(&self) {
        let cancelled = {
            let mut state = self.state();
            if let Some(task) = state.task.take() {
                task.abort();
            }
            state.asks.clear();
            state.invite = None;
            state.generation += 1;
            // Ending the session and recording why happen together: no one sees neither.
            let ended = state.view.session.take();
            if let Some(session) = &ended {
                state.view.last = Some(self.result(session.kind, None, "CANCELLED", false, None));
            }
            ended.is_some()
        };
        if cancelled {
            self.publish();
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn core(&self) -> MutexGuard<'_, Core> {
        self.inner
            .core
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> u64 {
        (self.inner.clock)()
    }

    fn publish(&self) {
        let view = self.view();
        self.inner.notify.view(&view);
    }

    /// Starts showing a session; refuses while another one runs.
    fn begin(&self, session: SessionView) -> Result<u64, SyncError> {
        let generation = {
            let mut state = self.state();
            if state.view.session.is_some() {
                return Err(SyncError("SESSION_RUNNING"));
            }
            state.generation += 1;
            state.view.session = Some(session);
            state.generation
        };
        self.publish();
        Ok(generation)
    }

    /// Changes the session in progress, unless it is not this one any more.
    fn update(&self, generation: u64, change: impl FnOnce(&mut SessionView)) {
        {
            let mut state = self.state();
            if state.generation != generation {
                return;
            }
            if let Some(session) = &mut state.view.session {
                change(session);
            }
        }
        self.publish();
    }

    /// Puts a question to the person and returns where the answer arrives.
    fn ask(
        &self,
        generation: u64,
        kind: AskKind,
        peer: &PeerInfo,
        sas: Option<String>,
    ) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let deadline_ms = self.now()
            + if kind == AskKind::Allow {
                ALLOW_MS
            } else {
                CONFIRM_MS
            };
        {
            let mut state = self.state();
            if state.generation != generation {
                return receiver;
            }
            state.next_ask += 1;
            let id = state.next_ask;
            state.asks.insert(id, sender);
            if let Some(session) = &mut state.view.session {
                session.phase = Phase::Asking;
                session.peer = Some(view::PeerView::of(peer));
                session.ask = Some(view::AskView {
                    id,
                    kind,
                    sas,
                    deadline_ms,
                });
            }
        }
        self.publish();
        receiver
    }

    fn keep_task(&self, generation: u64, task: tokio::task::AbortHandle) {
        let mut state = self.state();
        if state.generation == generation {
            state.task = Some(task);
        } else {
            task.abort();
        }
    }

    /// Saves what a session produced (when it produced something) and records how it ended.
    fn settle(&self, generation: u64, kind: SessionKind, peer: Option<&PeerInfo>, ended: Ended) {
        {
            let mut state = self.state();
            if state.generation != generation {
                return;
            }
            // The session stays shown while the result is saved, so a new one cannot start meanwhile.
            state.task = None;
            state.asks.clear();
            state.invite = None;
        }
        let (code, ok, entries) = match ended {
            Ended::Save {
                code,
                doc,
                overwritten,
                participants,
            } => {
                let mut core = self.core();
                match core.commit_session(*doc, &overwritten, &participants) {
                    Ok(()) => {
                        let count = core.list("").map(|l| l.len()).ok();
                        drop(core);
                        self.inner.notify.vault_changed();
                        (code, matches!(code, "SYNCED" | "PAIRED"), count)
                    }
                    Err(_) => ("SAVE_FAILED", false, None),
                }
            }
            Ended::Stop(code) => (code, false, None),
        };
        let result = self.result(kind, peer.map(view::PeerView::of), code, ok, entries);
        {
            let mut state = self.state();
            // Closed while saving: the data was saved all the same, so the result is still recorded.
            if state.generation == generation {
                state.view.session = None;
            }
            state.view.last = Some(result);
        }
        self.publish();
    }

    fn result(
        &self,
        kind: SessionKind,
        peer: Option<view::PeerView>,
        code: &'static str,
        ok: bool,
        entries: Option<usize>,
    ) -> ResultView {
        ResultView {
            kind,
            ok,
            code,
            peer,
            entries,
            at_ms: self.now(),
        }
    }

    /// Passes the window's events to the person: requests to allow (sync windows) and check codes to compare
    /// (pairing windows); anything else is declined. Returns the last device seen.
    async fn relay(
        &self,
        generation: u64,
        kind: SessionKind,
        mut events: tokio::sync::mpsc::Receiver<keep_sync::flow::sync_open::OpenerEvent>,
    ) -> Option<PeerInfo> {
        use keep_sync::flow::sync_open::OpenerEvent;
        let mut last_peer = None;
        while let Some(event) = events.recv().await {
            match event {
                OpenerEvent::Request { peer, answer } if kind == SessionKind::SyncWindow => {
                    self.forward(generation, AskKind::Allow, &peer, None, answer);
                    last_peer = Some(peer);
                }
                OpenerEvent::CheckCode { peer, sas, answer } if kind == SessionKind::PairWindow => {
                    self.forward(generation, AskKind::CheckCode, &peer, Some(sas), answer);
                    last_peer = Some(peer);
                }
                OpenerEvent::Request { answer, .. } | OpenerEvent::CheckCode { answer, .. } => {
                    let _ = answer.send(false);
                }
                OpenerEvent::Exchanging { peer } => {
                    self.update(generation, |s| {
                        s.phase = Phase::Exchanging;
                        s.ask = None;
                        s.peer = Some(view::PeerView::of(&peer));
                    });
                    last_peer = Some(peer);
                }
            }
        }
        last_peer
    }

    /// Asks the person and hands their answer to the session when it comes.
    fn forward(
        &self,
        generation: u64,
        kind: AskKind,
        peer: &PeerInfo,
        sas: Option<String>,
        answer: oneshot::Sender<bool>,
    ) {
        let receiver = self.ask(generation, kind, peer, sas);
        tokio::spawn(async move {
            let _ = answer.send(receiver.await.unwrap_or(false));
        });
    }

    /// The vault as it is now, for a window that reads it when a connection arrives.
    fn current_doc(&self, fallback: VaultDoc) -> Arc<dyn Fn() -> VaultDoc + Send + Sync> {
        let core = Arc::clone(&self.inner.core);
        Arc::new(move || {
            core.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .session_start()
                .map_or_else(|_| fallback.clone(), |start| start.doc)
        })
    }
}

#[cfg(test)]
mod tests;

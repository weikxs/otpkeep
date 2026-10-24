//! Finding a window and joining it: syncing with a member, or joining a group (design/06-sync.md §4.2, §5.3).

use keep_link::Transport;
use keep_link::device::{
    hello, lan_name, random_failed, target_from_invite, target_with_code, this_platform,
};
use keep_sync::channel::PROTOCOL_VERSION;
use keep_sync::channel::hello::Hello;
use keep_sync::flow::pair_join::{JoinMode, JoinPairOutcome, join_pair};
use keep_sync::flow::sync_join::{JoinOutcome, Joiner, join_sync};
use keep_sync::identity::NewDevice;
use keep_sync::lan::announce::{Announcement, matches_group};
use keep_sync::lan::window::{Timers, WindowKind};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::view::{PeerView, Phase, SessionKind, SessionView, stop_code, via_name};
use super::{AskKind, Ended, SyncError, Syncer, saved};

/// A window found on the network or over Bluetooth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundView {
    /// Names the window when joining it.
    pub index: usize,
    pub name: String,
    pub short_fingerprint: String,
    /// `lan` or `bluetooth`.
    pub via: &'static str,
    pub remaining_s: u32,
}

/// Which kind of window to look for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Looking {
    Sync,
    Pair,
}

/// How to reach a pairing window.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "by")]
pub enum PairWith {
    /// A window from the last search, with the eight-digit code shown on it.
    Found { index: usize, code: String },
    /// The text of the window's invite QR code.
    Invite { text: String },
}

impl Syncer {
    /// Searches for windows of this group (to sync) or any pairing window (to join a group) for up to 30
    /// seconds; the first ones found are returned a moment after they appear.
    ///
    /// # Errors
    ///
    /// Returns `NOT_IN_GROUP` (sync), `OTHER_GROUP` (pair, when already in a group) or `SESSION_RUNNING`.
    pub async fn search(&self, looking: Looking) -> Result<Vec<FoundView>, SyncError> {
        if self.state().view.session.is_some() {
            return Err(SyncError("SESSION_RUNNING"));
        }
        let start = self.core().session_start()?;
        let now = self.now();
        let found = match looking {
            Looking::Sync => {
                let group = start.doc.group.ok_or(SyncError("NOT_IN_GROUP"))?;
                let wanted = |a: &Announcement| {
                    a.group_tag.is_some_and(|tag| {
                        matches_group(&tag, &group.tag_key, &group.group_id, now)
                    })
                };
                self.inner
                    .network
                    .browse(Transport::Any, WindowKind::Sync, wanted)
                    .await?
            }
            Looking::Pair => {
                if start.doc.group.is_some() {
                    return Err(SyncError("OTHER_GROUP"));
                }
                self.inner
                    .network
                    .browse(Transport::Any, WindowKind::Pair, |_| true)
                    .await?
            }
        };
        let views = found
            .iter()
            .enumerate()
            .map(|(index, w)| FoundView {
                index,
                name: w.announcement.lan_name.clone(),
                short_fingerprint: w.announcement.short_fingerprint.clone(),
                via: via_name(w.route.via()),
                remaining_s: w.announcement.remaining_s,
            })
            .collect();
        self.state().found = found;
        Ok(views)
    }

    /// Syncs with a member's window from the last search, entering the six-digit code shown on it. The other
    /// person then allows the request.
    ///
    /// # Errors
    ///
    /// Returns `NOT_FOUND` for an unknown window, `NOT_IN_GROUP` or `SESSION_RUNNING`.
    pub fn join_sync(&self, index: usize, code: &str) -> Result<(), SyncError> {
        let window = self
            .state()
            .found
            .get(index)
            .cloned()
            .ok_or(SyncError("NOT_FOUND"))?;
        let start = self.core().session_start()?;
        let identity = start.identity.ok_or(SyncError("NOT_IN_GROUP"))?;
        let joiner = Joiner {
            me: identity.device_id(),
            exchange: identity.exchange_secret().clone(),
            hello: hello(&identity, &start.doc),
            timers: Timers::default(),
        };
        let generation = self.begin(joining(SessionKind::SyncJoin, &window.announcement))?;
        let code = Zeroizing::new(
            code.chars()
                .filter(char::is_ascii_digit)
                .collect::<String>(),
        );
        let me = identity.device_id();
        let this = self.clone();
        let task = tokio::spawn(async move {
            let clock = this.inner.clock.clone();
            let outcome = join_sync(&joiner, &window, code, &start.doc, move || clock()).await;
            let (peer, ended) = match outcome {
                JoinOutcome::Synced { peer, result } => {
                    let participants = [me, peer.device];
                    (
                        Some(peer),
                        saved("SYNCED", result.doc, result.overwritten, participants),
                    )
                }
                JoinOutcome::Removed { keep } => {
                    (None, saved("REMOVED", *keep, Vec::new(), [me, me]))
                }
                JoinOutcome::Busy => (None, Ended::Stop("BUSY")),
                JoinOutcome::WrongCode => (None, Ended::Stop("CODE_WRONG")),
                JoinOutcome::Unreachable => (None, Ended::Stop("NOT_FOUND")),
                JoinOutcome::Stopped(code) => (None, Ended::Stop(stop_code(code))),
                JoinOutcome::Lost => (None, Ended::Stop("SESSION_TIMEOUT")),
            };
            this.settle(generation, SessionKind::SyncJoin, peer.as_ref(), ended);
        });
        self.keep_task(generation, task.abort_handle());
        Ok(())
    }

    /// Joins a group through a pairing window. This device's entries are merged with the group's, or (with
    /// `replace`) backed up and dropped. The check code is then compared on both screens.
    ///
    /// # Errors
    ///
    /// Returns `OTHER_GROUP`, `INVALID_NAME`, `INVALID_INVITE`, `NOT_SAME_NETWORK`, `NOT_FOUND` or
    /// `SESSION_RUNNING`.
    pub fn join_pair(
        &self,
        with: &PairWith,
        name: Option<&str>,
        replace: bool,
    ) -> Result<(), SyncError> {
        let name = lan_name(name)?;
        let start = self.core().session_start()?;
        if start.doc.group.is_some() {
            return Err(SyncError("OTHER_GROUP"));
        }
        let (target, shown) = match with {
            PairWith::Found { index, code } => {
                let window = self
                    .state()
                    .found
                    .get(*index)
                    .cloned()
                    .ok_or(SyncError("NOT_FOUND"))?;
                let typed = Zeroizing::new(code.clone());
                (target_with_code(&window, &typed), Some(window.announcement))
            }
            PairWith::Invite { text } => (
                target_from_invite(text, &self.inner.network.addresses()?)?,
                None,
            ),
        };
        let new = NewDevice::generate().map_err(|_| SyncError::from(random_failed()))?;
        let me = new.device_id();
        let joiner_hello = Hello {
            device: me,
            dx_cert: None,
            platform: this_platform(),
            lan_name: name,
            versions: vec![PROTOCOL_VERSION],
        };
        let session = shown.as_ref().map_or_else(
            || SessionView {
                kind: SessionKind::PairJoin,
                phase: Phase::Connecting,
                code: None,
                has_invite: false,
                closes_at_ms: None,
                skipped: Vec::new(),
                peer: None,
                ask: None,
            },
            |a| joining(SessionKind::PairJoin, a),
        );
        let generation = self.begin(session)?;
        let had_entries = start
            .doc
            .entries
            .iter()
            .any(|e| e.trashed_at.value.is_none());
        let mode = if replace {
            JoinMode::Replace
        } else {
            JoinMode::Merge
        };
        let this = self.clone();
        let task = tokio::spawn(async move {
            let asking = this.clone();
            let decide = move |peer: keep_sync::flow::peer::PeerInfo, sas: String| {
                let receiver = asking.ask(generation, AskKind::CheckCode, &peer, Some(sas));
                async move { receiver.await.unwrap_or(false).then_some(mode) }
            };
            let clock = this.inner.clock.clone();
            let outcome = join_pair(
                &target,
                new,
                &joiner_hello,
                &start.doc,
                Timers::default(),
                move || clock(),
                decide,
            )
            .await;
            let (peer, ended) = match outcome {
                JoinPairOutcome::Paired {
                    peer,
                    result,
                    replaced,
                } => {
                    if replaced && had_entries && this.core().backup_before_pairing().is_err() {
                        tracing::warn!(
                            "the entries could not be backed up before they were replaced"
                        );
                    }
                    let participants = [me, peer.device];
                    (
                        Some(peer),
                        saved("PAIRED", result.doc, result.overwritten, participants),
                    )
                }
                JoinPairOutcome::Busy => (None, Ended::Stop("BUSY")),
                JoinPairOutcome::WrongCode => (None, Ended::Stop("CODE_WRONG")),
                JoinPairOutcome::Unreachable => (None, Ended::Stop("NOT_FOUND")),
                JoinPairOutcome::InAGroup => (None, Ended::Stop("OTHER_GROUP")),
                JoinPairOutcome::Stopped(code) => (None, Ended::Stop(stop_code(code))),
                JoinPairOutcome::Lost => (None, Ended::Stop("SESSION_TIMEOUT")),
            };
            this.settle(generation, SessionKind::PairJoin, peer.as_ref(), ended);
        });
        self.keep_task(generation, task.abort_handle());
        Ok(())
    }
}

/// A joining session, showing the window's device until the session names the peer.
fn joining(kind: SessionKind, announcement: &Announcement) -> SessionView {
    SessionView {
        kind,
        phase: Phase::Connecting,
        code: None,
        has_invite: false,
        closes_at_ms: None,
        skipped: Vec::new(),
        peer: Some(PeerView {
            name: announcement.lan_name.clone(),
            fingerprint: String::new(),
            short_fingerprint: announcement.short_fingerprint.clone(),
            platform: "",
        }),
        ask: None,
    }
}

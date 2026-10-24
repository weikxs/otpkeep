//! This device's windows: waiting for a member to sync, or for a new device to pair (design/06-sync.md
//! §4.1, §5.3).

use keep_link::Transport;
use keep_link::device::{hello, invite_text, lan_name, random_failed};
use keep_sync::flow::pair_open::{PairOutcome, PairWindow, run_pair_window};
use keep_sync::flow::sync_open::{SyncWindow, WindowOutcome, run_sync_window};
use keep_sync::identity::{DeviceIdentity, short_fingerprint};
use keep_sync::invite::{new_code, new_random};
use keep_sync::lan::announce::{Announcement, HOUR_MS, group_tag};
use keep_sync::lan::window::{Timers, WINDOW_WAIT_DEFAULT_MS, WindowKind};
use keep_transfer::qr::{QrEcc, qr_svg};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::view::{Phase, SessionKind, SessionView, SkippedView, close_code, stop_code, via_name};
use super::{Ended, SyncError, Syncer, saved};

const MINUTE_MS: u64 = 60 * 1000;

impl Syncer {
    /// Opens a sync window: shows a six-digit code and waits for a member of the group to enter it.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`], for example `NOT_IN_GROUP`, `SESSION_RUNNING` or `NO_ELIGIBLE_INTERFACE`.
    pub async fn open_sync_window(&self, minutes: Option<u64>) -> Result<(), SyncError> {
        let start = self.core().session_start()?;
        let identity = start.identity.ok_or(SyncError("NOT_IN_GROUP"))?;
        let group = start.doc.group.clone().ok_or(SyncError("NOT_IN_GROUP"))?;
        let wait_ms = minutes.map_or(WINDOW_WAIT_DEFAULT_MS, |m| m.clamp(1, 5) * MINUTE_MS);
        let code = new_code(6).map_err(|_| SyncError::from(random_failed()))?;
        let window_random = new_random().map_err(|_| SyncError::from(random_failed()))?;
        let now = self.now();
        let announcement = Announcement {
            group_tag: Some(group_tag(&group.tag_key, &group.group_id, now / HOUR_MS)),
            ..announcement(
                &identity,
                &start.doc,
                WindowKind::Sync,
                wait_ms,
                window_random,
            )
        };
        let generation = self.begin(window_view(SessionKind::SyncWindow, &code, now + wait_ms))?;
        let opened = match self.inner.network.open(Transport::Any, &announcement).await {
            Ok(opened) => opened,
            Err(error) => {
                self.settle(
                    generation,
                    SessionKind::SyncWindow,
                    None,
                    Ended::Stop(error.code),
                );
                return Err(error.into());
            }
        };
        let skipped = skipped_views(&opened.skipped);
        self.update(generation, |s| s.skipped = skipped);
        let window = SyncWindow {
            me: identity.device_id(),
            exchange: identity.exchange_secret().clone(),
            hello: hello(&identity, &start.doc),
            code,
            window_random,
            wait_ms,
            timers: Timers::default(),
        };
        let (events, asks) = mpsc::channel(4);
        let current = self.current_doc(start.doc);
        let wall = self.inner.clock.clone();
        let me = identity.device_id();
        let this = self.clone();
        let task = tokio::spawn(async move {
            let session = tokio::spawn(run_sync_window(
                window,
                opened.acceptor,
                (),
                current,
                wall,
                events,
            ));
            let last_peer = this.relay(generation, SessionKind::SyncWindow, asks).await;
            let (peer, ended) = match session.await {
                Ok(WindowOutcome::Synced { peer, result }) => {
                    let participants = [me, peer.device];
                    (
                        Some(peer),
                        saved("SYNCED", result.doc, result.overwritten, participants),
                    )
                }
                Ok(WindowOutcome::Removed { keep }) => {
                    (last_peer, saved("REMOVED", *keep, Vec::new(), [me, me]))
                }
                Ok(WindowOutcome::Closed(reason)) => (last_peer, Ended::Stop(close_code(reason))),
                Ok(WindowOutcome::Stopped(code)) => (last_peer, Ended::Stop(stop_code(code))),
                Err(_) => (last_peer, Ended::Stop("CANCELLED")),
            };
            this.settle(generation, SessionKind::SyncWindow, peer.as_ref(), ended);
        });
        self.keep_task(generation, task.abort_handle());
        Ok(())
    }

    /// Opens a pairing window: shows an eight-digit code (and an invite QR code on the local network) and waits
    /// for a new device. A device not yet in a group founds one first, named `name`.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`], for example `GROUP_FULL`, `INVALID_NAME` or `SESSION_RUNNING`.
    pub async fn open_pair_window(
        &self,
        name: Option<&str>,
        minutes: Option<u64>,
    ) -> Result<(), SyncError> {
        if self.state().view.session.is_some() {
            return Err(SyncError("SESSION_RUNNING"));
        }
        let name = lan_name(name)?;
        self.core().ensure_group(&name)?;
        let start = self.core().session_start()?;
        let identity = start.identity.ok_or(SyncError("NOT_IN_GROUP"))?;
        let group = start.doc.group.as_ref().ok_or(SyncError("NOT_IN_GROUP"))?;
        if keep_sync::membership::membership(group).is_full() {
            return Err(SyncError("GROUP_FULL"));
        }
        let wait_ms = minutes.map_or(WINDOW_WAIT_DEFAULT_MS, |m| m.clamp(1, 5) * MINUTE_MS);
        let code = new_code(8).map_err(|_| SyncError::from(random_failed()))?;
        let secret = Zeroizing::new(new_random().map_err(|_| SyncError::from(random_failed()))?);
        let window_random = new_random().map_err(|_| SyncError::from(random_failed()))?;
        let now = self.now();
        let announcement = announcement(
            &identity,
            &start.doc,
            WindowKind::Pair,
            wait_ms,
            window_random,
        );
        let generation = self.begin(window_view(SessionKind::PairWindow, &code, now + wait_ms))?;
        let opened = match self.inner.network.open(Transport::Any, &announcement).await {
            Ok(opened) => opened,
            Err(error) => {
                self.settle(
                    generation,
                    SessionKind::PairWindow,
                    None,
                    Ended::Stop(error.code),
                );
                return Err(error.into());
            }
        };
        let invite_svg = opened.lan.as_ref().and_then(|(addrs, port)| {
            let text = invite_text(addrs, *port, &announcement, &secret);
            qr_svg(&text, QrEcc::Medium)
                .ok()
                .map(|svg| svg.as_str().to_owned())
        });
        let skipped = skipped_views(&opened.skipped);
        let has_invite = invite_svg.is_some();
        self.state().invite = invite_svg;
        self.update(generation, |s| {
            s.skipped = skipped;
            s.has_invite = has_invite;
        });
        let window = PairWindow {
            me: identity.device_id(),
            exchange: identity.exchange_secret().clone(),
            hello: hello(&identity, &start.doc),
            code,
            secret,
            window_random,
            wait_ms,
            timers: Timers::default(),
        };
        let (events, asks) = mpsc::channel(4);
        let current = self.current_doc(start.doc);
        let wall = self.inner.clock.clone();
        let me = identity.device_id();
        let this = self.clone();
        let task = tokio::spawn(async move {
            let session = tokio::spawn(run_pair_window(
                window,
                opened.acceptor,
                (),
                current,
                wall,
                events,
            ));
            let last_peer = this.relay(generation, SessionKind::PairWindow, asks).await;
            let (peer, ended) = match session.await {
                Ok(PairOutcome::Paired { peer, result }) => {
                    let participants = [me, peer.device];
                    (
                        Some(peer),
                        saved("PAIRED", result.doc, result.overwritten, participants),
                    )
                }
                Ok(PairOutcome::Closed(reason)) => (last_peer, Ended::Stop(close_code(reason))),
                Ok(PairOutcome::Stopped(code)) => (last_peer, Ended::Stop(stop_code(code))),
                Err(_) => (last_peer, Ended::Stop("CANCELLED")),
            };
            this.settle(generation, SessionKind::PairWindow, peer.as_ref(), ended);
        });
        self.keep_task(generation, task.abort_handle());
        Ok(())
    }
}

fn announcement(
    identity: &DeviceIdentity,
    doc: &keep_vault::VaultDoc,
    kind: WindowKind,
    wait_ms: u64,
    window_random: [u8; 16],
) -> Announcement {
    Announcement {
        kind,
        lan_name: keep_link::device::own_name(doc, identity),
        short_fingerprint: short_fingerprint(identity.device_id()),
        remaining_s: u32::try_from(wait_ms / 1000).unwrap_or(u32::MAX),
        window_random,
        group_tag: None,
    }
}

fn window_view(kind: SessionKind, code: &str, closes_at_ms: u64) -> SessionView {
    SessionView {
        kind,
        phase: Phase::Waiting,
        code: Some(code.to_owned()),
        has_invite: false,
        closes_at_ms: Some(closes_at_ms),
        skipped: Vec::new(),
        peer: None,
        ask: None,
    }
}

fn skipped_views(skipped: &[keep_link::Skipped]) -> Vec<SkippedView> {
    skipped
        .iter()
        .map(|s| SkippedView {
            via: via_name(s.via),
            code: s.error.code,
        })
        .collect()
}

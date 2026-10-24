//! What the sync page shows: the session in progress and the last result (design/06-sync.md §8).

use keep_sync::channel::message::ErrorCode;
use keep_sync::flow::peer::PeerInfo;
use keep_sync::identity::short_fingerprint;
use keep_sync::lan::window::CloseReason;
use keep_sync::transport::Via;
use serde::Serialize;

use crate::core::sync::platform_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionKind {
    /// This device waits for a member to sync with it.
    SyncWindow,
    /// This device waits to add a new device.
    PairWindow,
    /// This device syncs with a member's window.
    SyncJoin,
    /// This device joins a group through a pairing window.
    PairJoin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// The window waits for a connection.
    Waiting,
    /// Connecting, checking the code, or waiting for the other person.
    Connecting,
    /// A question waits for this person.
    Asking,
    /// Data is being exchanged.
    Exchanging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AskKind {
    /// A member asks to sync: allow or decline (`SYNC_ALLOW`, 30 seconds).
    Allow,
    /// Pairing: does the other screen show the same check code (`PAIR_CONFIRM`, 60 seconds).
    CheckCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub name: String,
    pub fingerprint: String,
    pub short_fingerprint: String,
    pub platform: &'static str,
}

impl PeerView {
    pub fn of(peer: &PeerInfo) -> Self {
        Self {
            name: peer.lan_name.clone(),
            fingerprint: peer.fingerprint.clone(),
            short_fingerprint: short_fingerprint(peer.device),
            platform: platform_name(peer.platform),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskView {
    pub id: u64,
    pub kind: AskKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sas: Option<String>,
    pub deadline_ms: u64,
}

/// A transport the window does without.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedView {
    /// `lan` or `bluetooth`.
    pub via: &'static str,
    pub code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub kind: SessionKind,
    pub phase: Phase,
    /// The code shown on this screen: 6 digits to sync, 8 to pair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A pairing invite QR code can be shown (the local network is used). It holds the pairing secret, so it
    /// is handed out only after the person confirms natively.
    pub has_invite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at_ms: Option<u64>,
    pub skipped: Vec<SkippedView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<PeerView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<AskView>,
}

/// How the last session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultView {
    pub kind: SessionKind,
    pub ok: bool,
    /// `SYNCED`, `PAIRED`, or the reason (`CODE_WRONG`, `REJECTED`, `WINDOW_EXPIRED`, …).
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<PeerView>,
    /// Entries on this device afterwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<usize>,
    pub at_ms: u64,
}

/// Everything about sync the page needs besides the group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<ResultView>,
}

#[must_use]
pub const fn via_name(via: Via) -> &'static str {
    match via {
        Via::Lan => "lan",
        Via::Bluetooth => "bluetooth",
    }
}

/// The stable code for a window that closed (design/10-reference.md §4).
#[must_use]
pub const fn close_code(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Expired | CloseReason::TooManyConnections => "WINDOW_EXPIRED",
        CloseReason::CodeLocked => "CODE_LOCKED",
        CloseReason::ConfirmTimeout => "PAIR_CONFIRM_TIMEOUT",
        CloseReason::AllowTimeout => "ALLOW_TIMEOUT",
        CloseReason::DataIdle | CloseReason::SessionTimeout => "SESSION_TIMEOUT",
        CloseReason::Done | CloseReason::Cancelled => "CANCELLED",
    }
}

#[must_use]
pub fn stop_code(code: ErrorCode) -> &'static str {
    code.as_str()
}

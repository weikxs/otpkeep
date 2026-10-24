//! The joining side of pairing (design/06-sync.md §4.2–4.3): a device not in any group connects to a pairing
//! window, proves the code from the QR code or the screen, has its person compare the check code and choose
//! merge or replace, receives the group, certifies its new keys for it and runs the data phase.

use std::future::Future;
use std::time::Duration;

use keep_vault::{SyncGroup, VaultDoc};
use tokio::time::timeout;

use super::pair_open::{LINGER, receive_data, send_data};
use super::peer::PeerInfo;
use crate::channel::PROTOCOL_VERSION;
use crate::channel::code::{CodeCheck, CodeError, Secret, confirm_code};
use crate::channel::hello::{Hello, common_version};
use crate::channel::message::{ErrorCode, Message};
use crate::channel::noise::{Channel, ChannelError, Purpose, connect_noise};
use crate::group::join_group;
use crate::identity::{NewDevice, full_fingerprint, short_fingerprint, verify_dx_cert};
use crate::lan::window::Timers;
use crate::membership::membership;
use crate::session::data::Data;
use crate::session::exchange::{Exchanged, SessionError, exchange_as_joiner};
use crate::transport::{BoxLink, Route};

/// Extra time allowed beyond the opener's own timer, for the network.
const GRACE: Duration = Duration::from_secs(5);

/// What to do with this device's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinMode {
    /// Merge them with the group's (the default).
    Merge,
    /// Drop them; the caller keeps a backup first.
    Replace,
}

/// Where the window is and what proves the code.
pub struct PairTarget {
    /// How to reach the window.
    pub route: Route,
    /// The window's random value.
    pub window_random: [u8; 16],
    /// The opener's short fingerprint, which must match the device that answers.
    pub short_fingerprint: String,
    /// The secret from the QR code, or the typed code.
    pub secret: Secret,
}

/// How joining ended.
#[derive(Debug)]
pub enum JoinPairOutcome {
    /// Paired: commit `result.doc` (after a backup when `replaced`).
    Paired {
        /// The device that added this one.
        peer: PeerInfo,
        /// What to commit.
        result: Box<Exchanged>,
        /// Local entries were dropped.
        replaced: bool,
    },
    /// The window is busy with another device (`BUSY`).
    Busy,
    /// The code was wrong (`CODE_WRONG`).
    WrongCode,
    /// The window could not be reached.
    Unreachable,
    /// This device is already in a group (`OTHER_GROUP`).
    InAGroup,
    /// The session stopped with this code.
    Stopped(ErrorCode),
    /// The connection broke or went silent.
    Lost,
}

/// Joins a pairing window as a new member named `lan_name`. `decide` shows the opener and the check code to the
/// person and returns their choice, or `None` when the codes differ.
#[allow(clippy::too_many_lines, reason = "one session, step by step")]
pub async fn join_pair<D: Future<Output = Option<JoinMode>>>(
    target: &PairTarget,
    new: NewDevice,
    hello: &Hello,
    doc: &VaultDoc,
    timers: Timers,
    now_ms: impl Fn() -> u64,
    decide: impl FnOnce(PeerInfo, String) -> D,
) -> JoinPairOutcome {
    if doc.group.is_some() {
        return JoinPairOutcome::InAGroup;
    }
    let handshake = Duration::from_millis(timers.handshake_ms);
    let Some(stream) = target.route.connect(handshake).await else {
        return JoinPairOutcome::Unreachable;
    };
    let connected = timeout(
        handshake,
        connect_noise(
            stream,
            Purpose::Pair,
            &target.window_random,
            new.exchange_secret(),
            hello,
        ),
    )
    .await;
    let (mut channel, opener) = match connected {
        Ok(Ok(pair)) => pair,
        Ok(Err(ChannelError::Busy)) => return JoinPairOutcome::Busy,
        _ => return JoinPairOutcome::Lost,
    };
    if common_version(&[PROTOCOL_VERSION], &opener.versions).is_none() {
        return stop(&mut channel, ErrorCode::VersionMismatch).await;
    }
    if short_fingerprint(opener.device) != target.short_fingerprint {
        return stop(&mut channel, ErrorCode::NotInGroup).await;
    }
    let sas = match timeout(
        handshake,
        confirm_code(&mut channel, CodeCheck::Joiner(&target.secret)),
    )
    .await
    {
        Ok(Ok(confirmed)) => confirmed.sas,
        Ok(Err(CodeError::Wrong)) => return JoinPairOutcome::WrongCode,
        Ok(Err(CodeError::Stopped(code))) => return JoinPairOutcome::Stopped(code),
        _ => return JoinPairOutcome::Lost,
    };
    let peer = PeerInfo {
        device: opener.device,
        lan_name: opener.lan_name.clone(),
        platform: opener.platform,
        fingerprint: full_fingerprint(opener.device),
    };
    let Some(mode) = decide(peer.clone(), sas).await else {
        return stop(&mut channel, ErrorCode::SasMismatch).await;
    };
    if channel.send(&Message::Agreed.encode()).await.is_err() {
        return JoinPairOutcome::Lost;
    }
    let confirm_wait = Duration::from_millis(timers.pair_confirm_ms) + GRACE;
    if let Err(outcome) = wait_for_agreed(&mut channel, confirm_wait).await {
        return outcome;
    }
    let group = match receive_data(&mut channel).await {
        Ok(Data::Events(group_doc)) => match group_doc.group {
            Some(group) => group,
            None => return stop(&mut channel, ErrorCode::Protocol).await,
        },
        Ok(_) => return stop(&mut channel, ErrorCode::Protocol).await,
        Err(code) => return JoinPairOutcome::Stopped(code),
    };
    if !opener_belongs(&group, &opener, channel.remote_static()) {
        return stop(&mut channel, ErrorCode::NotInGroup).await;
    }
    let identity = new.into_identity(&group.group_id);
    if send_data(&mut channel, Data::Certificate(identity.dx_cert()))
        .await
        .is_err()
    {
        return JoinPairOutcome::Lost;
    }
    let replaced = mode == JoinMode::Replace;
    let Ok(joined) = join_group(doc, &identity, &group, &hello.lan_name, replaced, now_ms()) else {
        return stop(&mut channel, ErrorCode::OtherGroup).await;
    };
    channel.set_idle_timeout(Some(Duration::from_millis(timers.data_idle_ms)));
    match exchange_as_joiner(
        &mut channel,
        &joined,
        identity.device_id(),
        opener.device,
        now_ms(),
    )
    .await
    {
        Ok(result) => JoinPairOutcome::Paired {
            peer,
            result: Box::new(result),
            replaced,
        },
        Err(SessionError::Stopped(code)) => JoinPairOutcome::Stopped(code),
        Err(SessionError::Removed { .. }) => JoinPairOutcome::Stopped(ErrorCode::Removed),
        Err(SessionError::Lost) => JoinPairOutcome::Lost,
    }
}

/// The opener is a member of the group it sent, and its certificate binds the key it used in the handshake.
fn opener_belongs(group: &SyncGroup, opener: &Hello, remote_static: [u8; 32]) -> bool {
    let Some(cert) = opener.dx_cert else {
        return false;
    };
    let members = membership(group);
    members
        .members
        .get(&opener.device)
        .is_some_and(|m| m.dx_public == remote_static)
        && verify_dx_cert(opener.device, &group.group_id, &remote_static, &cert)
}

async fn stop(channel: &mut Channel<BoxLink>, code: ErrorCode) -> JoinPairOutcome {
    let _ = channel.send(&Message::Error(code).encode()).await;
    channel.close(LINGER).await;
    JoinPairOutcome::Stopped(code)
}

async fn wait_for_agreed(
    channel: &mut Channel<BoxLink>,
    limit: Duration,
) -> Result<(), JoinPairOutcome> {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let received = tokio::time::timeout_at(deadline, channel.recv())
            .await
            .map_err(|_| JoinPairOutcome::Stopped(ErrorCode::Cancelled))?
            .map_err(|_| JoinPairOutcome::Lost)?;
        match Message::decode(&received) {
            Some(Message::Keepalive) => {}
            Some(Message::Agreed) => return Ok(()),
            Some(Message::Error(code)) => return Err(JoinPairOutcome::Stopped(code)),
            _ => return Err(JoinPairOutcome::Stopped(ErrorCode::Protocol)),
        }
    }
}

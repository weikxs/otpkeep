//! The joiner's side of a sync (design/06-sync.md §5.3): connect to a found window, enter the code shown on
//! its screen, wait for the person there to allow, exchange.

use std::time::Duration;

use keep_vault::{DeviceId, VaultDoc};
use tokio::time::timeout;
use x25519_dalek::StaticSecret;
use zeroize::Zeroizing;

use super::peer::{PeerInfo, check_member};
use crate::channel::code::{CodeCheck, CodeError, Secret, confirm_code};
use crate::channel::hello::Hello;
use crate::channel::message::{ErrorCode, Message};
use crate::channel::noise::{Channel, ChannelError, Purpose, connect_noise};
use crate::lan::announce::FoundWindow;
use crate::lan::window::Timers;
use crate::session::exchange::{Exchanged, SessionError, exchange_as_joiner};
use crate::transport::BoxLink;

/// Extra time allowed beyond the opener's own timer, for the network.
const GRACE: Duration = Duration::from_secs(5);

/// How joining ended.
#[derive(Debug)]
pub enum JoinOutcome {
    /// Synced: commit `result.doc`.
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
    /// The opener is busy with another device (`BUSY`).
    Busy,
    /// The code was wrong (`CODE_WRONG`); the opener counts it.
    WrongCode,
    /// The window could not be reached.
    Unreachable,
    /// The session stopped with this code.
    Stopped(ErrorCode),
    /// The connection broke or went silent.
    Lost,
}

/// What joining needs.
pub struct Joiner {
    /// This device.
    pub me: DeviceId,
    /// Its X25519 secret.
    pub exchange: StaticSecret,
    /// Its introduction.
    pub hello: Hello,
    /// The session timers.
    pub timers: Timers,
}

/// Joins `window` with the code the person typed.
pub async fn join_sync(
    joiner: &Joiner,
    window: &FoundWindow,
    code: Zeroizing<String>,
    doc: &VaultDoc,
    now_ms: impl Fn() -> u64,
) -> JoinOutcome {
    let handshake = Duration::from_millis(joiner.timers.handshake_ms);
    let Some(stream) = window.route.connect(handshake).await else {
        return JoinOutcome::Unreachable;
    };
    let connected = timeout(
        handshake,
        connect_noise(
            stream,
            Purpose::Sync,
            &window.announcement.window_random,
            &joiner.exchange,
            &joiner.hello,
        ),
    )
    .await;
    let (mut channel, hello) = match connected {
        Ok(Ok(pair)) => pair,
        Ok(Err(ChannelError::Busy)) => return JoinOutcome::Busy,
        _ => return JoinOutcome::Lost,
    };
    let peer = match check_member(doc, &hello, channel.remote_static()) {
        Ok(peer) => peer,
        Err(code) => {
            let _ = channel.send(&Message::Error(code).encode()).await;
            return JoinOutcome::Stopped(code);
        }
    };
    let secret = Secret::SyncCode(code);
    match timeout(
        handshake,
        confirm_code(&mut channel, CodeCheck::Joiner(&secret)),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(CodeError::Wrong)) => return JoinOutcome::WrongCode,
        Ok(Err(CodeError::Stopped(code))) => return JoinOutcome::Stopped(code),
        _ => return JoinOutcome::Lost,
    }
    let allow_wait = Duration::from_millis(joiner.timers.sync_allow_ms) + GRACE;
    if let Err(outcome) = wait_for_allow(&mut channel, allow_wait).await {
        return outcome;
    }
    channel.set_idle_timeout(Some(Duration::from_millis(joiner.timers.data_idle_ms)));
    match exchange_as_joiner(&mut channel, doc, joiner.me, peer.device, now_ms()).await {
        Ok(result) => JoinOutcome::Synced {
            peer,
            result: Box::new(result),
        },
        Err(SessionError::Removed { keep }) => JoinOutcome::Removed { keep },
        Err(SessionError::Stopped(code)) => JoinOutcome::Stopped(code),
        Err(SessionError::Lost) => JoinOutcome::Lost,
    }
}

async fn wait_for_allow(
    channel: &mut Channel<BoxLink>,
    limit: Duration,
) -> Result<(), JoinOutcome> {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let received = tokio::time::timeout_at(deadline, channel.recv())
            .await
            .map_err(|_| JoinOutcome::Stopped(ErrorCode::Cancelled))?
            .map_err(|_| JoinOutcome::Lost)?;
        match Message::decode(&received) {
            Some(Message::Keepalive) => {}
            Some(Message::Agreed) => return Ok(()),
            Some(Message::Error(code)) => return Err(JoinOutcome::Stopped(code)),
            _ => return Err(JoinOutcome::Stopped(ErrorCode::Protocol)),
        }
    }
}

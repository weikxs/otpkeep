//! The opener's side of a pairing window (design/06-sync.md §4.3).
//!
//! The opener is already in a group (the caller founds one when opening the window). A connection goes through
//! the handshake, checks on the new device, the pairing code (the QR code's secret or the typed code) and the
//! check code both people compare; then the opener sends the group, receives the new device's certificate,
//! signs its join and runs the data phase. Nothing is sent about the group before both people agree.

use std::sync::Arc;
use std::time::Duration;

use keep_vault::{DeviceId, VaultDoc};
use tokio::sync::mpsc;
use x25519_dalek::StaticSecret;
use zeroize::Zeroizing;

use super::peer::PeerInfo;
use super::sync_open::{CurrentDoc, OpenerEvent, WallClock, ask_person};
use super::window_loop::{Progress, run_window};
use crate::channel::PROTOCOL_VERSION;
use crate::channel::code::{CodeCheck, CodeError, confirm_code};
use crate::channel::hello::{Hello, common_version};
use crate::channel::message::{ErrorCode, Message};
use crate::channel::noise::{Channel, Purpose, accept_noise};
use crate::group::{GroupError, admit};
use crate::identity::full_fingerprint;
use crate::lan::window::{CloseReason, Timers, WindowKind};
use crate::membership::membership;
use crate::session::data::{Data, partial};
use crate::session::exchange::{Exchanged, SessionError, exchange_as_opener};
use crate::transport::{Acceptor, BoxLink};

/// How long to wait for the other side to close after the last message.
pub(crate) const LINGER: Duration = Duration::from_secs(1);

/// How a pairing window ended.
#[derive(Debug)]
pub enum PairOutcome {
    /// Paired: the caller commits `result.doc`.
    Paired {
        /// The new device.
        peer: PeerInfo,
        /// What to commit.
        result: Box<Exchanged>,
    },
    /// The window closed without pairing.
    Closed(CloseReason),
    /// The session stopped with this code.
    Stopped(ErrorCode),
}

/// What the pairing window needs.
pub struct PairWindow {
    /// This device.
    pub me: DeviceId,
    /// Its X25519 secret.
    pub exchange: StaticSecret,
    /// Its introduction.
    pub hello: Hello,
    /// The 8-digit code on screen.
    pub code: Zeroizing<String>,
    /// The secret in the QR code.
    pub secret: Zeroizing<[u8; 16]>,
    /// The window's random value.
    pub window_random: [u8; 16],
    /// `WINDOW_WAIT`.
    pub wait_ms: u64,
    /// The session timers.
    pub timers: Timers,
}

/// Runs a pairing window until it closes.
pub async fn run_pair_window<A: Send>(
    window: PairWindow,
    acceptor: Acceptor,
    announcing: A,
    current: CurrentDoc,
    wall: WallClock,
    events: mpsc::Sender<OpenerEvent>,
) -> PairOutcome {
    let (wait_ms, timers) = (window.wait_ms, window.timers);
    let shared = Arc::new(window);
    let serve_one = |stream: BoxLink, progress: mpsc::Sender<Progress<PairOutcome>>| {
        tokio::spawn(serve(
            stream,
            Arc::clone(&shared),
            Arc::clone(&current),
            Arc::clone(&wall),
            events.clone(),
            progress,
        ))
    };
    run_window(
        WindowKind::Pair,
        wait_ms,
        timers,
        acceptor,
        announcing,
        serve_one,
    )
    .await
    .unwrap_or_else(PairOutcome::Closed)
}

async fn serve(
    stream: BoxLink,
    window: Arc<PairWindow>,
    current: CurrentDoc,
    wall: WallClock,
    events: mpsc::Sender<OpenerEvent>,
    progress: mpsc::Sender<Progress<PairOutcome>>,
) {
    let report = |p: Progress<PairOutcome>| {
        let progress = progress.clone();
        async move {
            let _ = progress.send(p).await;
        }
    };
    let Ok((mut channel, hello)) = accept_noise(
        stream,
        Purpose::Pair,
        &window.window_random,
        &window.exchange,
        &window.hello,
    )
    .await
    else {
        return report(Progress::Dropped).await;
    };
    if let Err(code) = check_newcomer(&current(), &hello) {
        let _ = channel.send(&Message::Error(code).encode()).await;
        report(Progress::Dropped).await;
        channel.close(LINGER).await;
        return;
    }
    let check = CodeCheck::PairOpener {
        secret: &window.secret,
        code: &window.code,
    };
    let sas = match confirm_code(&mut channel, check).await {
        Ok(confirmed) => confirmed.sas,
        Err(CodeError::Wrong) => return report(Progress::CodeFailed).await,
        Err(_) => return report(Progress::Dropped).await,
    };
    report(Progress::CodePassed).await;
    let peer = PeerInfo {
        device: hello.device,
        lan_name: hello.lan_name.clone(),
        platform: hello.platform,
        fingerprint: full_fingerprint(hello.device),
    };
    let outcome = pair(
        &mut channel,
        &window,
        &peer,
        sas,
        &hello,
        &current,
        &wall,
        &events,
        &report,
    )
    .await;
    channel.close(LINGER).await;
    report(Progress::Finished(outcome)).await;
}

/// After a right code: both people confirm the check code, then the group, the certificate, the join and
/// the data phase.
#[allow(
    clippy::too_many_arguments,
    reason = "one step of one session, split out for length"
)]
async fn pair<F: std::future::Future<Output = ()>>(
    channel: &mut Channel<BoxLink>,
    window: &PairWindow,
    peer: &PeerInfo,
    sas: String,
    hello: &Hello,
    current: &CurrentDoc,
    wall: &WallClock,
    events: &mpsc::Sender<OpenerEvent>,
    report: &impl Fn(Progress<PairOutcome>) -> F,
) -> PairOutcome {
    let check = |answer| OpenerEvent::CheckCode {
        peer: peer.clone(),
        sas,
        answer,
    };
    if !ask_person(channel, check, events, window.timers.keepalive_ms).await {
        let _ = channel
            .send(&Message::Error(ErrorCode::SasMismatch).encode())
            .await;
        return PairOutcome::Stopped(ErrorCode::SasMismatch);
    }
    if let Err(code) = expect_agreed(channel).await {
        return PairOutcome::Stopped(code);
    }
    if channel.send(&Message::Agreed.encode()).await.is_err() {
        return PairOutcome::Stopped(ErrorCode::Cancelled);
    }
    report(Progress::Agreed).await;
    let doc = current();
    let Some(group) = doc.group.clone() else {
        return PairOutcome::Stopped(ErrorCode::NotInGroup);
    };
    let group_doc = partial(Vec::new(), Vec::new(), Some(group), doc.clock);
    if send_data(channel, Data::Events(group_doc)).await.is_err() {
        return PairOutcome::Stopped(ErrorCode::Cancelled);
    }
    let cert = match receive_data(channel).await {
        Ok(Data::Certificate(cert)) => cert,
        Ok(_) => return PairOutcome::Stopped(ErrorCode::Protocol),
        Err(code) => return PairOutcome::Stopped(code),
    };
    let admitted = match admit(
        &doc,
        hello.device,
        channel.remote_static(),
        &cert,
        hello.platform,
        wall(),
    ) {
        Ok(admitted) => admitted,
        Err(error) => {
            let code = group_error(error);
            let _ = channel.send(&Message::Error(code).encode()).await;
            return PairOutcome::Stopped(code);
        }
    };
    let _ = events
        .send(OpenerEvent::Exchanging { peer: peer.clone() })
        .await;
    channel.set_idle_timeout(Some(Duration::from_millis(window.timers.data_idle_ms)));
    match exchange_as_opener(channel, &admitted, window.me, hello.device, wall()).await {
        Ok(result) => PairOutcome::Paired {
            peer: peer.clone(),
            result: Box::new(result),
        },
        Err(SessionError::Stopped(code)) => PairOutcome::Stopped(code),
        Err(SessionError::Removed { .. }) => PairOutcome::Stopped(ErrorCode::Removed),
        Err(SessionError::Lost) => PairOutcome::Stopped(ErrorCode::Cancelled),
    }
}

/// The new device must speak a common version and be neither removed nor already a member; the group must
/// have room.
fn check_newcomer(doc: &VaultDoc, hello: &Hello) -> Result<(), ErrorCode> {
    if common_version(&[PROTOCOL_VERSION], &hello.versions).is_none() {
        return Err(ErrorCode::VersionMismatch);
    }
    let group = doc.group.as_ref().ok_or(ErrorCode::NotInGroup)?;
    let members = membership(group);
    if members.is_removed(hello.device) {
        return Err(ErrorCode::Removed);
    }
    if members.is_member(hello.device) {
        return Err(ErrorCode::AlreadyMember);
    }
    if members.is_full() {
        return Err(ErrorCode::GroupFull);
    }
    Ok(())
}

fn group_error(error: GroupError) -> ErrorCode {
    match error {
        GroupError::Full => ErrorCode::GroupFull,
        GroupError::AlreadyMember => ErrorCode::AlreadyMember,
        GroupError::Removed => ErrorCode::Removed,
        GroupError::AlreadyInGroup => ErrorCode::OtherGroup,
        GroupError::NotInGroup | GroupError::BadCertificate => ErrorCode::NotInGroup,
        GroupError::Random | GroupError::NotAMember | GroupError::BadName => ErrorCode::Cancelled,
    }
}

/// The joiner's person decided first; its decision is waiting.
async fn expect_agreed(channel: &mut Channel<BoxLink>) -> Result<(), ErrorCode> {
    loop {
        let bytes = channel.recv().await.map_err(|_| ErrorCode::Cancelled)?;
        match Message::decode(&bytes) {
            Some(Message::Agreed) => return Ok(()),
            Some(Message::Keepalive) => {}
            Some(Message::Error(code)) => return Err(code),
            _ => return Err(ErrorCode::Protocol),
        }
    }
}

pub(crate) async fn send_data(channel: &mut Channel<BoxLink>, data: Data) -> Result<(), ErrorCode> {
    channel
        .send(&Message::Data(data.encode()).encode())
        .await
        .map_err(|_| ErrorCode::Cancelled)
}

pub(crate) async fn receive_data(channel: &mut Channel<BoxLink>) -> Result<Data, ErrorCode> {
    loop {
        let bytes = channel.recv().await.map_err(|_| ErrorCode::Cancelled)?;
        match Message::decode(&bytes) {
            Some(Message::Data(inner)) => return Data::decode(&inner).ok_or(ErrorCode::Protocol),
            Some(Message::Keepalive) => {}
            Some(Message::Error(code)) => return Err(code),
            _ => return Err(ErrorCode::Protocol),
        }
    }
}

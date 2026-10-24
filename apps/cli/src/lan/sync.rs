//! `otpkeep sync wait` and `otpkeep sync join` (design/06-sync.md §5).

use std::io::Write;
use std::sync::Arc;

use keep_sync::channel::PROTOCOL_VERSION;
use keep_sync::channel::hello::Hello;
use keep_sync::flow::listen::Listeners;
use keep_sync::flow::peer::PeerInfo;
use keep_sync::flow::sync_join::{JoinOutcome, Joiner, join_sync};
use keep_sync::flow::sync_open::{OpenerEvent, SyncWindow, WindowOutcome, run_sync_window};
use keep_sync::group::identity_of;
use keep_sync::identity::{DeviceIdentity, short_fingerprint};
use keep_sync::invite::{new_code, new_random};
use keep_sync::lan::announce::{Announcement, HOUR_MS, group_tag, matches_group};
use keep_sync::lan::window::{CloseReason, Timers, WINDOW_WAIT_DEFAULT_MS, WindowKind};
use keep_sync::membership::membership;
use keep_vault::VaultDoc;
use keep_vault::service::VaultService;
use serde_json::json;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::{not_synced, pick, runtime, stopped, this_platform};
use crate::args::Options;
use crate::commands::{Printed, service_error};
use crate::open::open_service;
use crate::{CliError, Context, EXIT_CANCELLED, Prompt};

const MINUTE_MS: u64 = 60 * 1000;

/// Opens a sync window and waits for a member; the person here allows the request.
pub(crate) fn sync_wait(
    context: &mut Context<'_>,
    minutes: Option<u64>,
    options: Options,
    out: &mut dyn Write,
) -> Result<Printed, CliError> {
    require_terminal(context, "allow a sync request")?;
    let mut service = open_service(context, options)?;
    let identity = identity_of(service.doc()).map_err(|_| not_in_group())?;
    let doc = service.doc().clone();
    let network = context.network.clone();
    let wait_ms = minutes.map_or(WINDOW_WAIT_DEFAULT_MS, |m| m * MINUTE_MS);
    let code = new_code(6).map_err(|_| random_failed())?;
    let window_random = new_random().map_err(|_| random_failed())?;
    let now = context.now_ms;
    let rt = runtime()?;
    let outcome = rt.block_on(async {
        let addrs = network.addresses()?;
        let listeners = Listeners::bind(&addrs).await.map_err(|_| {
            not_synced(
                "NO_ELIGIBLE_INTERFACE",
                "the window could not start listening",
            )
        })?;
        let group = doc.group.as_ref().ok_or_else(not_in_group)?;
        let announcement = Announcement {
            kind: WindowKind::Sync,
            lan_name: own_name(&doc, &identity),
            short_fingerprint: short_fingerprint(identity.device_id()),
            remaining_s: u32::try_from(wait_ms / 1000).unwrap_or(u32::MAX),
            window_random,
            group_tag: Some(group_tag(&group.tag_key, &group.group_id, now() / HOUR_MS)),
        };
        let announced = network.announce(&addrs, listeners.port(), &announcement)?;
        let mut acceptor = keep_sync::transport::Acceptor::new();
        acceptor.add_lan(listeners);
        let _ = writeln!(
            out,
            "Sync code: {} {}   (waiting up to {} min; enter it on the other device)",
            &code[..3],
            &code[3..],
            wait_ms / MINUTE_MS
        );
        let window = SyncWindow {
            me: identity.device_id(),
            exchange: identity.exchange_secret().clone(),
            hello: hello(&identity, &doc),
            code,
            window_random,
            wait_ms,
            timers: Timers::default(),
        };
        let (events, mut asks) = mpsc::channel(4);
        let current_doc = doc.clone();
        let session = tokio::spawn(run_sync_window(
            window,
            acceptor,
            announced,
            Arc::new(move || current_doc.clone()),
            Arc::new(now),
            events,
        ));
        while let Some(event) = asks.recv().await {
            match event {
                OpenerEvent::Request { peer, answer } => {
                    let question = format!(
                        "{} requests to sync. Allow? (within 30 seconds)",
                        describe(&peer)
                    );
                    let allowed = matches!(
                        (context.prompt)(Prompt::Confirm(question))
                            .map(|a| a.trim().to_lowercase())
                            .as_deref(),
                        Some("y" | "yes")
                    );
                    let _ = answer.send(allowed);
                }
                OpenerEvent::Exchanging { peer } => {
                    let _ = writeln!(out, "Syncing with {} …", describe(&peer));
                }
                OpenerEvent::CheckCode { answer, .. } => {
                    let _ = answer.send(false);
                }
            }
        }
        session
            .await
            .map_err(|_| stopped(keep_sync::channel::message::ErrorCode::Cancelled))
    })?;
    match outcome {
        WindowOutcome::Synced { peer, result } => {
            commit(&mut service, &peer, result.doc, &result.overwritten)
        }
        WindowOutcome::Removed { keep } => Err(removed(&mut service, *keep)),
        WindowOutcome::Closed(reason) => Err(closed(reason)),
        WindowOutcome::Stopped(code) => Err(stopped(code)),
    }
}

/// Finds this group's sync window and joins it with the code shown there.
pub(crate) fn sync_join(
    context: &mut Context<'_>,
    to: Option<&str>,
    options: Options,
    out: &mut dyn Write,
) -> Result<Printed, CliError> {
    require_terminal(context, "type the sync code")?;
    let mut service = open_service(context, options)?;
    let identity = identity_of(service.doc()).map_err(|_| not_in_group())?;
    let doc = service.doc().clone();
    let group = doc.group.clone().ok_or_else(not_in_group)?;
    let network = context.network.clone();
    let now = context.now_ms;
    let rt = runtime()?;
    let _ = writeln!(out, "Searching for devices waiting to sync …");
    let windows = rt.block_on(network.browse(WindowKind::Sync, |a| {
        a.group_tag
            .is_some_and(|tag| matches_group(&tag, &group.tag_key, &group.group_id, now()))
    }))?;
    let window = pick(context, &windows, to)?.clone();
    let typed = (context.prompt)(Prompt::Text(format!(
        "Sync code shown on {} · {}:",
        window.announcement.lan_name, window.announcement.short_fingerprint
    )))
    .ok_or_else(cancelled)?;
    let code = Zeroizing::new(
        typed
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>(),
    );
    let joiner = Joiner {
        me: identity.device_id(),
        exchange: identity.exchange_secret().clone(),
        hello: hello(&identity, &doc),
        timers: Timers::default(),
    };
    let _ = writeln!(
        out,
        "Waiting for {} to allow …",
        window.announcement.lan_name
    );
    let outcome = rt.block_on(join_sync(&joiner, &window, code, &doc, now));
    match outcome {
        JoinOutcome::Synced { peer, result } => {
            commit(&mut service, &peer, result.doc, &result.overwritten)
        }
        JoinOutcome::Removed { keep } => Err(removed(&mut service, *keep)),
        JoinOutcome::Busy => Err(stopped(keep_sync::channel::message::ErrorCode::Busy)),
        JoinOutcome::WrongCode => Err(stopped(keep_sync::channel::message::ErrorCode::CodeWrong)),
        JoinOutcome::Unreachable => Err(not_synced(
            "NOT_FOUND",
            "the waiting device could not be reached",
        )),
        JoinOutcome::Stopped(code) => Err(stopped(code)),
        JoinOutcome::Lost => Err(not_synced(
            "SESSION_TIMEOUT",
            "the connection was lost; nothing was changed",
        )),
    }
}

pub(super) fn commit(
    service: &mut VaultService,
    peer: &PeerInfo,
    doc: VaultDoc,
    overwritten: &[(
        keep_vault::EntryId,
        keep_vault::history::HistoryField,
        String,
    )],
) -> Result<Printed, CliError> {
    let before = service.doc().entries.len();
    service
        .commit_synced(doc, overwritten)
        .map_err(service_error)?;
    let after = service.doc().entries.len();
    Ok(Printed {
        text: format!(
            "Synced with {}: {} now (was {}).\n",
            describe(peer),
            super::entries(after),
            super::entries(before)
        ),
        json: json!({ "peer": peer.lan_name, "fingerprint": peer.fingerprint, "entries": after }),
        notes: Vec::new(),
    })
}

fn removed(service: &mut VaultService, keep: VaultDoc) -> CliError {
    let _ = service.commit_synced(keep, &[]);
    stopped(keep_sync::channel::message::ErrorCode::Removed)
}

pub(super) fn closed(reason: CloseReason) -> CliError {
    match reason {
        CloseReason::Expired => not_synced(
            "WINDOW_EXPIRED",
            "nobody connected in time; the window closed",
        ),
        CloseReason::CodeLocked => stopped(keep_sync::channel::message::ErrorCode::CodeLocked),
        CloseReason::TooManyConnections => {
            not_synced("WINDOW_EXPIRED", "too many connections; the window closed")
        }
        CloseReason::ConfirmTimeout => not_synced(
            "PAIR_CONFIRM_TIMEOUT",
            "the check code was not confirmed within 60 seconds",
        ),
        CloseReason::AllowTimeout => not_synced(
            "ALLOW_TIMEOUT",
            "the request was not allowed within 30 seconds",
        ),
        CloseReason::DataIdle | CloseReason::SessionTimeout => not_synced(
            "SESSION_TIMEOUT",
            "the session timed out; nothing was changed",
        ),
        CloseReason::Done | CloseReason::Cancelled => {
            stopped(keep_sync::channel::message::ErrorCode::Cancelled)
        }
    }
}

/// This device's introduction.
pub(super) fn hello(identity: &DeviceIdentity, doc: &VaultDoc) -> Hello {
    Hello {
        device: identity.device_id(),
        dx_cert: Some(identity.dx_cert()),
        platform: this_platform(),
        lan_name: own_name(doc, identity),
        versions: vec![PROTOCOL_VERSION],
    }
}

/// This device's LAN name in its group.
fn own_name(doc: &VaultDoc, identity: &DeviceIdentity) -> String {
    doc.group
        .as_ref()
        .and_then(|g| {
            membership(g)
                .members
                .get(&identity.device_id())
                .and_then(|m| m.lan_name.clone())
        })
        .unwrap_or_else(|| format!("OTPKeep-{}", short_fingerprint(identity.device_id())))
}

pub(super) fn describe(peer: &PeerInfo) -> String {
    format!(
        "{} · {} ({})",
        peer.lan_name,
        peer.fingerprint,
        super::platform_name(peer.platform)
    )
}

pub(super) fn require_terminal(context: &Context<'_>, what: &str) -> Result<(), CliError> {
    if context.stdin_is_tty {
        return Ok(());
    }
    Err(CliError::new(
        EXIT_CANCELLED,
        "CANCELLED",
        &format!("run this in a terminal: a person must {what}"),
    ))
}

fn not_in_group() -> CliError {
    not_synced(
        "NOT_IN_GROUP",
        "this device is not in a sync group; add it with 'otpkeep pair join', or start one with 'otpkeep pair open'",
    )
}

pub(super) fn random_failed() -> CliError {
    not_synced(
        "RANDOM",
        "the system random number generator is unavailable",
    )
}

pub(super) fn cancelled() -> CliError {
    CliError::new(
        EXIT_CANCELLED,
        "CANCELLED",
        "cancelled; nothing was changed",
    )
}

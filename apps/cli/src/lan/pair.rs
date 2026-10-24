//! `otpkeep pair open` and `otpkeep pair join` (design/06-sync.md §4).

use std::io::Write;
use std::sync::Arc;

use keep_sync::channel::PROTOCOL_VERSION;
use keep_sync::channel::code::Secret;
use keep_sync::channel::hello::{Hello, is_valid_lan_name};
use keep_sync::flow::listen::Listeners;
use keep_sync::flow::pair_join::{JoinMode, JoinPairOutcome, PairTarget, join_pair};
use keep_sync::flow::pair_open::{PairOutcome, PairWindow, run_pair_window};
use keep_sync::flow::sync_open::OpenerEvent;
use keep_sync::group::{found_group, identity_of};
use keep_sync::identity::{NewDevice, short_fingerprint};
use keep_sync::invite::{PairInvite, new_code, new_random};
use keep_sync::lan::announce::Announcement;
use keep_sync::lan::interfaces::{Eligible, peer_is_on_link};
use keep_sync::lan::window::{Timers, WINDOW_WAIT_DEFAULT_MS, WindowKind};
use keep_sync::membership::membership;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use super::sync::{cancelled, closed, commit, describe, hello, random_failed, require_terminal};
use super::{not_synced, pick, runtime, stopped, this_platform};
use crate::args::Options;
use crate::commands::{Printed, service_error};
use crate::open::open_service;
use crate::{CliError, Context, EXIT_INVALID_INPUT, Prompt};

const MINUTE_MS: u64 = 60 * 1000;

/// Opens a pairing window, founding a sync group first when this device has none.
pub(crate) fn pair_open(
    context: &mut Context<'_>,
    minutes: Option<u64>,
    name: Option<&str>,
    qr: bool,
    options: Options,
    out: &mut dyn Write,
) -> Result<Printed, CliError> {
    require_terminal(context, "compare the check code")?;
    let mut service = open_service(context, options)?;
    if service.doc().group.is_none() {
        let name = lan_name(name)?;
        let grouped = found_group(service.doc(), this_platform(), &name, (context.now_ms)())
            .map_err(|error| not_synced("GROUP", &error.to_string()))?;
        service.commit_synced(grouped, &[]).map_err(service_error)?;
        let _ = writeln!(out, "Started a new sync group; this device is \"{name}\".");
    }
    let doc = service.doc().clone();
    let identity = identity_of(&doc).map_err(|error| not_synced("GROUP", &error.to_string()))?;
    if membership(doc.group.as_ref().ok_or_else(cancelled)?).is_full() {
        return Err(stopped(keep_sync::channel::message::ErrorCode::GroupFull));
    }
    let network = context.network.clone();
    let wait_ms = minutes.map_or(WINDOW_WAIT_DEFAULT_MS, |m| m * MINUTE_MS);
    let code = new_code(8).map_err(|_| random_failed())?;
    let secret = Zeroizing::new(new_random().map_err(|_| random_failed())?);
    let window_random = new_random().map_err(|_| random_failed())?;
    let now = context.now_ms;
    let rt = runtime()?;
    let outcome = rt.block_on(async {
        let addrs = network.addresses()?;
        let listeners = Listeners::bind(&addrs)
            .await
            .map_err(|_| not_synced("NO_ELIGIBLE_INTERFACE", "the window could not start listening"))?;
        let opener_hello = hello(&identity, &doc);
        let announcement = Announcement {
            kind: WindowKind::Pair,
            lan_name: opener_hello.lan_name.clone(),
            short_fingerprint: short_fingerprint(identity.device_id()),
            remaining_s: u32::try_from(wait_ms / 1000).unwrap_or(u32::MAX),
            window_random,
            group_tag: None,
        };
        let announced = network.announce(&addrs, listeners.port(), &announcement)?;
        let _ = writeln!(
            out,
            "Pairing code: {} {}   (waiting up to {} min; enter it on the new device with 'otpkeep pair join')",
            &code[..4],
            &code[4..],
            wait_ms / MINUTE_MS
        );
        if qr {
            show_invite(out, &addrs, listeners.port(), &announcement, &secret);
        }
        let window = PairWindow {
            me: identity.device_id(),
            exchange: identity.exchange_secret().clone(),
            hello: opener_hello,
            code,
            secret,
            window_random,
            wait_ms,
            timers: Timers::default(),
        };
        let (events, mut asks) = mpsc::channel(4);
        let current_doc = doc.clone();
        let session = tokio::spawn(run_pair_window(
            window,
            listeners,
            announced,
            Arc::new(move || current_doc.clone()),
            Arc::new(now),
            events,
        ));
        while let Some(event) = asks.recv().await {
            match event {
                OpenerEvent::CheckCode { peer, sas, answer } => {
                    let question = format!(
                        "Adding {}. Does the other device show the check code {} {}? (within 60 seconds)",
                        describe(&peer),
                        &sas[..3],
                        &sas[3..]
                    );
                    let _ = answer.send(yes((context.prompt)(Prompt::Confirm(question))));
                }
                OpenerEvent::Exchanging { peer } => {
                    let _ = writeln!(out, "Adding {} and exchanging entries …", describe(&peer));
                }
                OpenerEvent::Request { answer, .. } => {
                    let _ = answer.send(false);
                }
            }
        }
        session.await.map_err(|_| stopped(keep_sync::channel::message::ErrorCode::Cancelled))
    })?;
    match outcome {
        PairOutcome::Paired { peer, result } => {
            commit(&mut service, &peer, result.doc, &result.overwritten)
        }
        PairOutcome::Closed(reason) => Err(closed(reason)),
        PairOutcome::Stopped(code) => Err(stopped(code)),
    }
}

/// Joins a sync group through a pairing window: from a QR code's text, or found on the network with the
/// typed pairing code.
#[allow(clippy::too_many_arguments, reason = "the command's flags")]
pub(crate) fn pair_join(
    context: &mut Context<'_>,
    invite: Option<&str>,
    to: Option<&str>,
    name: Option<&str>,
    replace: bool,
    options: Options,
    out: &mut dyn Write,
) -> Result<Printed, CliError> {
    require_terminal(context, "compare the check code")?;
    let mut service = open_service(context, options)?;
    if service.doc().group.is_some() {
        return Err(stopped(keep_sync::channel::message::ErrorCode::OtherGroup));
    }
    let name = lan_name(name)?;
    let doc = service.doc().clone();
    let network = context.network.clone();
    let rt = runtime()?;
    let target = if let Some(text) = invite {
        from_invite(text, &network.addresses()?)?
    } else {
        let _ = writeln!(out, "Searching for devices waiting to add one …");
        let windows = rt.block_on(network.browse(WindowKind::Pair, |_| true))?;
        found_target(context, &windows, to)?
    };
    let new = NewDevice::generate().map_err(|_| random_failed())?;
    let joiner_hello = Hello {
        device: new.device_id(),
        dx_cert: None,
        platform: this_platform(),
        lan_name: name,
        versions: vec![PROTOCOL_VERSION],
    };
    let has_entries = !doc.entries.is_empty();
    let prompt = context.prompt;
    let decide = |peer: keep_sync::flow::peer::PeerInfo, sas: String| {
        let question = format!(
            "Joining {}. Does it show the check code {} {}? (within 60 seconds)",
            describe(&peer),
            &sas[..3],
            &sas[3..]
        );
        let agreed = yes(prompt(Prompt::Confirm(question)));
        let mode = if replace {
            JoinMode::Replace
        } else {
            JoinMode::Merge
        };
        std::future::ready(agreed.then_some(mode))
    };
    if has_entries && !replace {
        let _ = writeln!(
            out,
            "This device's entries will be merged with the group's (use --replace to drop them)."
        );
    }
    let outcome = rt.block_on(join_pair(
        &target,
        new,
        &joiner_hello,
        &doc,
        Timers::default(),
        context.now_ms,
        decide,
    ));
    match outcome {
        JoinPairOutcome::Paired {
            peer,
            result,
            replaced,
        } => {
            if replaced && has_entries {
                let path = service.backup_before_pairing().map_err(service_error)?;
                let _ = writeln!(
                    out,
                    "Backed up this device's entries to {} before replacing them.",
                    path.display()
                );
            }
            commit(&mut service, &peer, result.doc, &result.overwritten)
        }
        JoinPairOutcome::Busy => Err(stopped(keep_sync::channel::message::ErrorCode::Busy)),
        JoinPairOutcome::WrongCode => {
            Err(stopped(keep_sync::channel::message::ErrorCode::CodeWrong))
        }
        JoinPairOutcome::Unreachable => Err(not_synced(
            "NOT_FOUND",
            "the waiting device could not be reached",
        )),
        JoinPairOutcome::InAGroup => {
            Err(stopped(keep_sync::channel::message::ErrorCode::OtherGroup))
        }
        JoinPairOutcome::Stopped(code) => Err(stopped(code)),
        JoinPairOutcome::Lost => Err(not_synced(
            "SESSION_TIMEOUT",
            "the connection was lost; nothing was changed",
        )),
    }
}

/// The window the person picks, with the pairing code they type.
fn found_target(
    context: &Context<'_>,
    windows: &[keep_sync::lan::announce::FoundWindow],
    to: Option<&str>,
) -> Result<PairTarget, CliError> {
    let window = pick(context, windows, to)?.clone();
    let typed = (context.prompt)(Prompt::Text(format!(
        "Pairing code shown on {} · {}:",
        window.announcement.lan_name, window.announcement.short_fingerprint
    )))
    .ok_or_else(cancelled)?;
    Ok(PairTarget {
        addrs: window.addrs,
        port: window.port,
        window_random: window.announcement.window_random,
        short_fingerprint: window.announcement.short_fingerprint,
        secret: Secret::PairCode(Zeroizing::new(
            typed.chars().filter(char::is_ascii_digit).collect(),
        )),
    })
}

fn yes(answer: Option<String>) -> bool {
    matches!(
        answer.map(|a| a.trim().to_lowercase()).as_deref(),
        Some("y" | "yes")
    )
}

/// The name given, or `OTPKeep-` and four random hex digits.
fn lan_name(name: Option<&str>) -> Result<String, CliError> {
    match name {
        Some(name) if is_valid_lan_name(name) => Ok(name.to_owned()),
        Some(_) => Err(CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "the name must have 1 to 32 characters without surrounding spaces",
        )),
        None => {
            let random = new_random().map_err(|_| random_failed())?;
            Ok(format!("OTPKeep-{:02X}{:02X}", random[0], random[1]))
        }
    }
}

/// The invite, kept to addresses on this device's links.
fn from_invite(text: &str, addrs: &[Eligible]) -> Result<PairTarget, CliError> {
    let invite = PairInvite::parse(text)
        .map_err(|error| CliError::new(EXIT_INVALID_INPUT, "INVALID_INPUT", &error.to_string()))?;
    let reachable: Vec<_> = invite
        .addrs
        .iter()
        .filter(|addr| {
            addrs
                .iter()
                .any(|local| peer_is_on_link(local, **addr, None))
        })
        .map(|addr| (*addr, None))
        .collect();
    if reachable.is_empty() {
        return Err(not_synced(
            "NOT_SAME_NETWORK",
            "the pairing code points to another network",
        ));
    }
    Ok(PairTarget {
        addrs: reachable,
        port: invite.port,
        window_random: invite.window_random,
        short_fingerprint: invite.short_fingerprint,
        secret: Secret::PairSecret(invite.secret),
    })
}

/// Prints the pairing invite as text and as a QR code drawn in the terminal.
fn show_invite(
    out: &mut dyn Write,
    addrs: &[Eligible],
    port: u16,
    announcement: &Announcement,
    secret: &[u8; 16],
) {
    let invite = PairInvite {
        addrs: addrs
            .iter()
            .map(|a| a.addr)
            .filter(std::net::IpAddr::is_ipv4)
            .collect(),
        port,
        short_fingerprint: announcement.short_fingerprint.clone(),
        window_random: announcement.window_random,
        secret: *secret,
    };
    let text = invite.encode();
    let _ = writeln!(
        out,
        "Invite (it pairs without the code: keep it private):\n  otpkeep pair join --invite '{}'",
        &*text
    );
    if let Ok(code) =
        qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
    {
        let drawn = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        let _ = writeln!(out, "{drawn}");
    }
}

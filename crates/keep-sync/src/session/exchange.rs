//! The data phase over the channel (design/06-sync.md §5.4 ⑥).
//!
//! The joiner speaks first at every step and the opener answers, so neither side ever waits on a full buffer
//! while the other does the same:
//!
//! 1. membership events; both check that both devices are members (a removal stops the session, and both keep
//!    the merged events);
//! 2. a hash of every entry;
//! 3. the entries the other side lacks or holds differently, and all tombstones, in batches;
//! 4. the opener merges and removes duplicate keys and sends what that changed; the joiner merges the same;
//! 5. both state digests; only equal digests let the caller commit.
//!
//! Timestamps from the other side more than a day ahead stop the session (`PEER_CLOCK`). Nothing is written
//! here: the caller commits the returned state.

use std::collections::BTreeMap;

use keep_vault::history::HistoryField;
use keep_vault::{
    DeviceId, Entry, EntryId, Hlc, SyncGroup, Tombstone, VaultDoc, encode_sync_state,
};
use tokio::io::{AsyncRead, AsyncWrite};

use super::data::{Data, partial};
use super::{next_stamp, overwritten, share_membership, with_presence};
use crate::channel::MAX_MESSAGE;
use crate::channel::message::{ErrorCode, Message};
use crate::channel::noise::Channel;
use crate::digest::{check_peer_clock, state_digest};
use crate::housekeeping::dedupe;
use crate::membership::{membership, merge_events};
use crate::merge::merge;

/// Aim for batches well under the message limit.
const BATCH_TARGET: usize = MAX_MESSAGE / 2;

/// Why the data phase stopped. Nothing is to be written, except `keep` after a removal.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// A device was removed (`REMOVED`): write `keep`, which adds only the membership events.
    #[error("a device was removed from the sync group")]
    Removed {
        /// This side's state with both sides' membership events.
        keep: Box<VaultDoc>,
    },
    /// This side or the other stopped with this code.
    #[error("the sync stopped: {}", .0.as_str())]
    Stopped(ErrorCode),
    /// The connection failed.
    #[error("the connection was lost")]
    Lost,
}

/// What this side commits.
#[derive(Debug, Clone)]
pub struct Exchanged {
    /// The new state.
    pub doc: VaultDoc,
    /// Entries moved to the trash as duplicates (opener only).
    pub deduped: Vec<EntryId>,
    /// The digest both sides agreed on.
    pub digest: [u8; 32],
    /// Names and notes the merge replaced, for this side's edit history.
    pub overwritten: Vec<(EntryId, HistoryField, String)>,
}

/// The opener's data phase.
///
/// # Errors
///
/// Returns [`SessionError`].
pub async fn exchange_as_opener<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    ours: &VaultDoc,
    me: DeviceId,
    peer: DeviceId,
    now_ms: u64,
) -> Result<Exchanged, SessionError> {
    let participants = [me, peer];
    let their_group = expect_events(channel).await?;
    refuse_far_future(
        channel,
        &partial(vec![], vec![], Some(their_group.clone()), ours.clock),
        now_ms,
    )
    .await?;
    send(channel, Data::Events(events_of(ours)?)).await?;
    check_members(ours, &their_group, &participants)?;
    let their_summary = expect_summary(channel).await?;
    send(channel, Data::Summary(summary(ours))).await?;
    let (entries, tombstones) = receive_entries(channel).await?;
    let received = partial(entries, tombstones, Some(their_group), ours.clock);
    refuse_far_future(channel, &received, now_ms).await?;
    send_entries(
        channel,
        missing_for(ours, &their_summary),
        ours.tombstones.clone(),
    )
    .await?;
    let merged = merge(ours, &received, &participants);
    let (cleaned, deduped) = dedupe(&merged, now_ms);
    send_entries(channel, changed(&merged, &cleaned), Vec::new()).await?;
    let doc = with_presence(cleaned, &participants, next_stamp(&merged, now_ms));
    let digest = state_digest(&doc);
    let theirs = expect_digest(channel).await?;
    send(channel, Data::Digest(digest)).await?;
    agree(theirs, digest)?;
    Ok(Exchanged {
        overwritten: overwritten(ours, &doc),
        doc,
        deduped,
        digest,
    })
}

/// The joiner's data phase.
///
/// # Errors
///
/// Returns [`SessionError`].
pub async fn exchange_as_joiner<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    ours: &VaultDoc,
    me: DeviceId,
    peer: DeviceId,
    now_ms: u64,
) -> Result<Exchanged, SessionError> {
    let participants = [peer, me];
    send(channel, Data::Events(events_of(ours)?)).await?;
    let their_group = expect_events(channel).await?;
    refuse_far_future(
        channel,
        &partial(vec![], vec![], Some(their_group.clone()), ours.clock),
        now_ms,
    )
    .await?;
    check_members(ours, &their_group, &participants)?;
    send(channel, Data::Summary(summary(ours))).await?;
    let their_summary = expect_summary(channel).await?;
    send_entries(
        channel,
        missing_for(ours, &their_summary),
        ours.tombstones.clone(),
    )
    .await?;
    let (entries, tombstones) = receive_entries(channel).await?;
    let received = partial(entries, tombstones, Some(their_group), ours.clock);
    refuse_far_future(channel, &received, now_ms).await?;
    let merged = merge(ours, &received, &participants);
    let (changes, _) = receive_entries(channel).await?;
    let merged = merge(
        &merged,
        &partial(changes, Vec::new(), None, merged.clock),
        &participants,
    );
    let doc = with_presence(merged.clone(), &participants, next_stamp(&merged, now_ms));
    let digest = state_digest(&doc);
    send(channel, Data::Digest(digest)).await?;
    let theirs = expect_digest(channel).await?;
    agree(theirs, digest)?;
    Ok(Exchanged {
        overwritten: overwritten(ours, &doc),
        doc,
        deduped: Vec::new(),
        digest,
    })
}

fn events_of(doc: &VaultDoc) -> Result<VaultDoc, SessionError> {
    let group = doc
        .group
        .clone()
        .ok_or(SessionError::Stopped(ErrorCode::NotInGroup))?;
    Ok(partial(Vec::new(), Vec::new(), Some(group), doc.clock))
}

/// Both must be members by everything both know; a removal keeps the merged events.
fn check_members(
    ours: &VaultDoc,
    theirs: &SyncGroup,
    participants: &[DeviceId; 2],
) -> Result<(), SessionError> {
    let group = ours
        .group
        .as_ref()
        .ok_or(SessionError::Stopped(ErrorCode::NotInGroup))?;
    if group.group_id != theirs.group_id {
        return Err(SessionError::Stopped(ErrorCode::NotInGroup));
    }
    let events = merge_events(&group.group_id, &group.events, &theirs.events);
    let members = membership(&SyncGroup {
        events,
        ..group.clone()
    });
    if participants.iter().any(|d| members.is_removed(*d)) {
        let their_doc = partial(Vec::new(), Vec::new(), Some(theirs.clone()), ours.clock);
        let (keep, _) = share_membership(ours, &their_doc);
        return Err(SessionError::Removed {
            keep: Box::new(keep),
        });
    }
    if !participants.iter().all(|d| members.is_member(*d)) {
        return Err(SessionError::Stopped(ErrorCode::NotInGroup));
    }
    Ok(())
}

async fn refuse_far_future<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    received: &VaultDoc,
    now_ms: u64,
) -> Result<(), SessionError> {
    if check_peer_clock(received, now_ms).is_ok() {
        return Ok(());
    }
    let _ = channel
        .send(&Message::Error(ErrorCode::PeerClock).encode())
        .await;
    Err(SessionError::Stopped(ErrorCode::PeerClock))
}

/// A hash of each entry's complete state.
fn summary(doc: &VaultDoc) -> Vec<(EntryId, [u8; 32])> {
    doc.entries.iter().map(|e| (e.id, entry_hash(e))).collect()
}

fn entry_hash(entry: &Entry) -> [u8; 32] {
    *blake3::hash(&encode_sync_state(&[entry], &[], &[])).as_bytes()
}

/// Our entries the other side lacks or holds differently.
fn missing_for(ours: &VaultDoc, their_summary: &[(EntryId, [u8; 32])]) -> Vec<Entry> {
    let theirs: BTreeMap<EntryId, [u8; 32]> = their_summary.iter().copied().collect();
    ours.entries
        .iter()
        .filter(|e| theirs.get(&e.id) != Some(&entry_hash(e)))
        .cloned()
        .collect()
}

/// Entries of `after` that differ from `before`.
fn changed(before: &VaultDoc, after: &VaultDoc) -> Vec<Entry> {
    let before: BTreeMap<EntryId, &Entry> = before.entries.iter().map(|e| (e.id, e)).collect();
    after
        .entries
        .iter()
        .filter(|e| before.get(&e.id) != Some(e))
        .cloned()
        .collect()
}

async fn send_entries<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    entries: Vec<Entry>,
    tombstones: Vec<Tombstone>,
) -> Result<(), SessionError> {
    let clock = Hlc {
        wall_ms: 0,
        logical: 0,
        device: keep_vault::DeviceTag([0; 8]),
    };
    let mut batch: Vec<Entry> = Vec::new();
    let mut size = 0;
    let mut tombstones = Some(tombstones);
    for entry in entries {
        let entry_size = encode_sync_state(&[&entry], &[], &[]).len();
        if !batch.is_empty() && size + entry_size > BATCH_TARGET {
            let full = std::mem::take(&mut batch);
            send(
                channel,
                Data::Entries(partial(
                    full,
                    tombstones.take().unwrap_or_default(),
                    None,
                    clock,
                )),
            )
            .await?;
            size = 0;
        }
        size += entry_size;
        batch.push(entry);
    }
    if !batch.is_empty() || tombstones.as_ref().is_some_and(|t| !t.is_empty()) {
        send(
            channel,
            Data::Entries(partial(
                batch,
                tombstones.take().unwrap_or_default(),
                None,
                clock,
            )),
        )
        .await?;
    }
    send(channel, Data::Done).await
}

async fn receive_entries<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<(Vec<Entry>, Vec<Tombstone>), SessionError> {
    let (mut entries, mut tombstones) = (Vec::new(), Vec::new());
    loop {
        match receive(channel).await? {
            Data::Entries(batch) => {
                entries.extend(batch.entries);
                tombstones.extend(batch.tombstones);
            }
            Data::Done => return Ok((entries, tombstones)),
            _ => return Err(protocol(channel).await),
        }
    }
}

async fn expect_events<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<SyncGroup, SessionError> {
    match receive(channel).await? {
        Data::Events(doc) => match doc.group {
            Some(group) => Ok(group),
            None => Err(protocol(channel).await),
        },
        _ => Err(protocol(channel).await),
    }
}

async fn expect_summary<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<Vec<(EntryId, [u8; 32])>, SessionError> {
    match receive(channel).await? {
        Data::Summary(items) => Ok(items),
        _ => Err(protocol(channel).await),
    }
}

async fn expect_digest<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<[u8; 32], SessionError> {
    match receive(channel).await? {
        Data::Digest(digest) => Ok(digest),
        _ => Err(protocol(channel).await),
    }
}

fn agree(theirs: [u8; 32], ours: [u8; 32]) -> Result<(), SessionError> {
    if theirs == ours {
        return Ok(());
    }
    tracing::error!("state digests differ after the sync");
    Err(SessionError::Stopped(ErrorCode::DigestMismatch))
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
    data: Data,
) -> Result<(), SessionError> {
    let message = Message::Data(data.encode());
    channel
        .send(&message.encode())
        .await
        .map_err(|error| match error {
            crate::channel::noise::ChannelError::TooLarge => {
                SessionError::Stopped(ErrorCode::TooLarge)
            }
            _ => SessionError::Lost,
        })
}

async fn receive<S: AsyncRead + AsyncWrite + Unpin>(
    channel: &mut Channel<S>,
) -> Result<Data, SessionError> {
    loop {
        let bytes = channel.recv().await.map_err(|error| match error {
            crate::channel::noise::ChannelError::TooLarge => {
                SessionError::Stopped(ErrorCode::TooLarge)
            }
            _ => SessionError::Lost,
        })?;
        match Message::decode(&bytes) {
            Some(Message::Data(inner)) => {
                return Data::decode(&inner).ok_or(SessionError::Stopped(ErrorCode::Protocol));
            }
            Some(Message::Keepalive) => {}
            Some(Message::Error(code)) => return Err(SessionError::Stopped(code)),
            _ => return Err(SessionError::Stopped(ErrorCode::Protocol)),
        }
    }
}

async fn protocol<S: AsyncRead + AsyncWrite + Unpin>(channel: &mut Channel<S>) -> SessionError {
    let _ = channel
        .send(&Message::Error(ErrorCode::Protocol).encode())
        .await;
    SessionError::Stopped(ErrorCode::Protocol)
}

#[cfg(test)]
mod tests;

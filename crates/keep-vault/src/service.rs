//! `VaultService`: the one way every surface (app window, browser page, command line) reads and changes the
//! vault (design/01-architecture.md §3).
//!
//! Each change is computed as a new document, saved, and only then becomes current, so a rejected change or a
//! failed save leaves everything as it was. Name and note changes are also recorded in this device's history.

use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use keep_passcode::{
    CodeKind, CodeSpec, Moment, Passcode, PasscodeError, SecretKey, derive_passcode,
};

use crate::edit::{self, Change, EditError, NewEntry, Placement};
use crate::history::{HistoryField, HistoryVersion, LocalDoc, decode_local, encode_local};
use crate::model::{Entry, EntryId, VaultDoc};
use crate::open::{OpenVault, SaveError};
use crate::store::write_private;

/// Why a service call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    /// The change was rejected; nothing changed.
    #[error(transparent)]
    Edit(#[from] EditError),
    /// The change could not be saved; nothing changed.
    #[error(transparent)]
    Save(#[from] SaveError),
    /// No passcode can be computed for this entry.
    #[error(transparent)]
    Passcode(#[from] PasscodeError),
    /// No entry with that id.
    #[error("the entry does not exist")]
    NotFound,
    /// An HOTP entry has no code until the next code is asked for (PRD FR-1.5).
    #[error("ask for the next code first")]
    NoCodeYet,
    /// The change was saved, but its history could not be.
    #[error("the change was saved, but its history could not be")]
    HistoryNotSaved,
}

/// Milliseconds since the Unix epoch, from the system clock.
#[must_use]
pub fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The open vault plus this device's history and the HOTP codes generated in this session.
pub struct VaultService {
    open: OpenVault,
    local: LocalDoc,
    now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
    /// HOTP codes live only in memory: they are gone after a restart or a lock (design/03 §1).
    hotp_codes: HashMap<EntryId, Passcode>,
}

impl std::fmt::Debug for VaultService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultService")
            .field("entries", &self.open.doc().entries.len())
            .finish_non_exhaustive()
    }
}

impl VaultService {
    /// Wraps an open vault. A damaged `local.okv` is set aside and the history starts empty: history must never
    /// keep anyone from their codes.
    pub fn new(open: OpenVault, now_ms: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        let local = load_local(&open);
        Self {
            open,
            local,
            now_ms: Box::new(now_ms),
            hotp_codes: HashMap::new(),
        }
    }

    /// The vault.
    #[must_use]
    pub fn doc(&self) -> &VaultDoc {
        self.open.doc()
    }

    /// One entry, including entries in the trash.
    #[must_use]
    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.doc().entries.iter().find(|e| e.id == id)
    }

    /// Entries not in the trash, in display order.
    #[must_use]
    pub fn ordered(&self) -> Vec<&Entry> {
        edit::ordered(self.doc())
    }

    /// Entries in the trash, most recent first.
    #[must_use]
    pub fn trashed(&self) -> Vec<&Entry> {
        edit::trashed(self.doc())
    }

    /// Earlier versions of an entry's name or note, newest first.
    #[must_use]
    pub fn history(&self, id: EntryId, field: HistoryField) -> Vec<&HistoryVersion> {
        self.local.versions(id, field)
    }

    /// Adds an entry.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn add(&mut self, new: NewEntry) -> Result<EntryId, ServiceError> {
        let (doc, id) = edit::add_entry(self.doc(), new, self.now())?;
        self.commit(doc)?;
        Ok(id)
    }

    /// Changes a name, note, tags, issuer, account or pin, recording the old name or note in the history.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; only [`ServiceError::HistoryNotSaved`] means the change itself was saved.
    pub fn edit(&mut self, id: EntryId, change: Change) -> Result<(), ServiceError> {
        let now = self.now();
        let before = self.entry(id).ok_or(ServiceError::NotFound)?;
        let previous = match &change {
            Change::Title(_) => Some((HistoryField::Title, before.title.value.clone())),
            Change::Note(_) => Some((HistoryField::Note, before.note.value.clone())),
            _ => None,
        };
        let doc = edit::edit_entry(self.doc(), id, change, now)?;
        self.commit(doc)?;
        match previous {
            Some((field, old)) if self.current_text(id, field) != Some(old.as_str()) => {
                self.save_local(self.local.record(id, field, &old, now))
            }
            _ => Ok(()),
        }
    }

    /// Moves an entry to the trash.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn trash(&mut self, id: EntryId) -> Result<(), ServiceError> {
        let doc = edit::move_to_trash(self.doc(), id, self.now())?;
        self.commit(doc)
    }

    /// Takes an entry out of the trash.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn restore(&mut self, id: EntryId) -> Result<(), ServiceError> {
        let doc = edit::restore(self.doc(), id, self.now())?;
        self.commit(doc)
    }

    /// Deletes an entry permanently, with its history.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn purge(&mut self, id: EntryId) -> Result<(), ServiceError> {
        let doc = edit::purge(self.doc(), id)?;
        self.commit(doc)?;
        self.hotp_codes.remove(&id);
        self.save_local(self.local.forget(id))
    }

    /// Deletes entries that have been in the trash for more than 30 days.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn purge_expired_trash(&mut self) -> Result<(), ServiceError> {
        let doc = edit::purge_expired_trash(self.doc(), self.now());
        let removed: Vec<EntryId> = self
            .doc()
            .entries
            .iter()
            .map(|e| e.id)
            .filter(|id| !doc.entries.iter().any(|e| e.id == *id))
            .collect();
        if removed.is_empty() {
            return Ok(());
        }
        self.commit(doc)?;
        let local = removed
            .iter()
            .fold(self.local.clone(), |local, id| local.forget(*id));
        for id in &removed {
            self.hotp_codes.remove(id);
        }
        self.save_local(local)
    }

    /// Moves an entry in the manual order.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn move_entry(&mut self, id: EntryId, placement: Placement) -> Result<(), ServiceError> {
        let doc = edit::move_entry(self.doc(), id, placement, self.now())?;
        self.commit(doc)
    }

    /// Gives an entry a new secret; the old one stays in the trash as a copy.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn replace_key(
        &mut self,
        id: EntryId,
        secret: SecretKey,
        spec: CodeSpec,
        counter: u64,
    ) -> Result<(), ServiceError> {
        let doc = edit::replace_key(self.doc(), id, secret, spec, counter, self.now())?;
        self.commit(doc)?;
        self.hotp_codes.remove(&id);
        Ok(())
    }

    /// The current passcode. TOTP codes follow the clock; an HOTP entry returns the code last generated with
    /// [`VaultService::next_hotp`] in this session, without changing the counter.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::NotFound`], [`ServiceError::NoCodeYet`] or [`ServiceError::Passcode`].
    pub fn passcode(&self, id: EntryId) -> Result<Passcode, ServiceError> {
        let key = &self.entry(id).ok_or(ServiceError::NotFound)?.key.value;
        match key.spec.kind() {
            CodeKind::TimeBased { .. } => {
                let seconds = i64::try_from(self.now() / 1000).unwrap_or(i64::MAX);
                Ok(derive_passcode(
                    &key.secret,
                    key.spec,
                    Moment::Time(seconds),
                )?)
            }
            CodeKind::CounterBased => {
                if !key.spec.digest().can_compute() {
                    return Err(PasscodeError::UnsupportedDigest(key.spec.digest()).into());
                }
                self.hotp_codes
                    .get(&id)
                    .cloned()
                    .ok_or(ServiceError::NoCodeYet)
            }
        }
    }

    /// Generates the next HOTP code: computes the code for the stored counter, then saves the counter plus one.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`]; nothing changes on failure.
    pub fn next_hotp(&mut self, id: EntryId) -> Result<Passcode, ServiceError> {
        let key = &self.entry(id).ok_or(ServiceError::NotFound)?.key.value;
        if !key.spec.digest().can_compute() {
            return Err(PasscodeError::UnsupportedDigest(key.spec.digest()).into());
        }
        let (doc, next_counter) = edit::next_hotp(self.doc(), id, self.now())?;
        let used = doc
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| &e.key.value)
            .ok_or(ServiceError::NotFound)?;
        let code = derive_passcode(
            &used.secret,
            used.spec,
            Moment::Counter(next_counter.saturating_sub(1)),
        )?;
        self.commit(doc)?;
        self.hotp_codes.insert(id, code.clone());
        Ok(code)
    }

    fn now(&self) -> u64 {
        (self.now_ms)()
    }

    fn commit(&mut self, doc: VaultDoc) -> Result<(), ServiceError> {
        Ok(self.open.save(doc)?)
    }

    fn current_text(&self, id: EntryId, field: HistoryField) -> Option<&str> {
        let entry = self.entry(id)?;
        Some(match field {
            HistoryField::Title => &entry.title.value,
            HistoryField::Note => &entry.note.value,
        })
    }

    fn save_local(&mut self, local: LocalDoc) -> Result<(), ServiceError> {
        let sealed = self
            .open
            .seal_local(&encode_local(&local))
            .map_err(|_| ServiceError::HistoryNotSaved)?;
        write_private(&self.open.paths().local(), &sealed)
            .map_err(|_| ServiceError::HistoryNotSaved)?;
        self.local = local;
        Ok(())
    }
}

fn load_local(open: &OpenVault) -> LocalDoc {
    let path = open.paths().local();
    let Ok(bytes) = fs::read(&path) else {
        return LocalDoc::default();
    };
    if let Some(local) = open
        .open_local(&bytes)
        .and_then(|plaintext| decode_local(&plaintext).ok())
    {
        return local;
    }
    tracing::warn!("history file damaged; set aside, history starts empty");
    // Keep the damaged file aside for diagnosis; if even that fails, it is overwritten by the next history save.
    let mut aside = path.into_os_string();
    aside.push(format!(".damaged-{}", system_now_ms()));
    let _ = fs::rename(open.paths().local(), aside);
    LocalDoc::default()
}

#[cfg(test)]
mod tests;

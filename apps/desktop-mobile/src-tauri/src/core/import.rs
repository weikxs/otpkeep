//! Adding accounts: pasted text, files and pictures, and the manual form all become a staged import that
//! the person reviews and commits (PRD FR-2.1–2.3, FR-6.6). A first commit creates the vault.

use keep_transfer::backup::BackupError;
use keep_transfer::import::{Candidate, Choice, ImportError, Origin, commit_import, preview};
use keep_transfer::source::{Source, SourceError, read_source, read_text};
use keep_vault::VaultDoc;
use keep_vault::open::{CreateError, create_vault};
use serde::Deserialize;
use zeroize::Zeroize;

use super::import_view::{ImportView, view};
use super::key::{read_spec, read_typed_secret};
use super::{Core, CoreError, Vault};
use crate::staging::Staged;

/// What the manual form sends (PRD FR-2.2). The secret is typed by the person, so it does come in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualForm {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub issuer: String,
    #[serde(default)]
    pub account: String,
    pub secret: String,
    /// `totp`, `hotp` or `steam`.
    pub kind: String,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    #[serde(default = "default_digits")]
    pub digits: u8,
    #[serde(default = "default_period")]
    pub period_s: u16,
    #[serde(default)]
    pub counter: u64,
}

impl Drop for ManualForm {
    /// The typed secret is wiped when the form is dropped.
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

fn default_algorithm() -> String {
    "SHA1".to_owned()
}

const fn default_digits() -> u8 {
    keep_passcode::DEFAULT_DIGITS
}

const fn default_period() -> u16 {
    keep_passcode::DEFAULT_PERIOD_S
}

/// Why something could not be staged or committed, for the window to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportProblem {
    /// An OTPKeep backup: ask for its password and read it again.
    NeedsBackupPassword,
    WrongBackupPassword,
    BackupDamaged,
    BackupNewer,
    ImageTooLarge,
    UnknownFormat,
    /// The staged import is gone (committed, cancelled or too old).
    NotStaged,
    /// The vault changed since the preview; the import was previewed again under the same token.
    Changed,
    /// No system keychain to hold a new vault's key; a master password is needed.
    NoKeystore,
}

impl Core {
    /// Stages pasted or typed text: one link, several, or transfer codes.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the data is not open or no token can be made.
    pub fn stage_text(&mut self, text: &str) -> Result<ImportView, CoreError> {
        self.stage(read_text(text))
    }

    /// Stages a file or picture; a backup needs `password`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Import`] when the source cannot be read at all.
    pub fn stage_bytes(
        &mut self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<ImportView, CoreError> {
        let source =
            read_source(bytes, password).map_err(|e| CoreError::Import(source_problem(e)))?;
        self.stage(source)
    }

    /// Stages one account typed into the form; the view shows its current code to compare with the website.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] or [`CoreError::SecretInvalid`] for a field that is not acceptable.
    pub fn stage_manual(&mut self, form: &ManualForm) -> Result<ImportView, CoreError> {
        let candidate = manual_candidate(form)?;
        self.stage(Source {
            candidates: vec![candidate],
            skipped: Vec::new(),
        })
    }

    /// The view of a staged import, for example after [`ImportProblem::Changed`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Import`] with [`ImportProblem::NotStaged`] when it is gone.
    pub fn staged_view(&mut self, token: &str) -> Result<ImportView, CoreError> {
        let now = (self.clock)();
        let doc = self.preview_doc()?;
        let staged = self
            .staging
            .get(token, now)
            .ok_or(CoreError::Import(ImportProblem::NotStaged))?;
        Ok(view(token, staged, &doc, now))
    }

    /// Adds the chosen rows, creating the vault first if there is none. Returns how many were added.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`]; nothing is added on failure. When the vault changed since the preview, the
    /// import is previewed again under the same token and [`ImportProblem::Changed`] is returned.
    pub fn commit_staged(&mut self, token: &str, choice: &Choice) -> Result<usize, CoreError> {
        let now = (self.clock)();
        let staged = self
            .staging
            .take(token, now)
            .ok_or(CoreError::Import(ImportProblem::NotStaged))?;
        if matches!(self.vault, Vault::Fresh) {
            self.create()?;
        }
        let Vault::Open { service, read_only } = &mut self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        if *read_only {
            return Err(CoreError::ReadOnly);
        }
        let current = preview(staged.candidates.clone(), service.doc());
        if current.fresh != staged.preview.fresh || current.duplicates != staged.preview.duplicates
        {
            let again = Staged::new(staged.candidates, current, staged.skipped, now);
            let _ = self.staging.put_back(token, again);
            return Err(CoreError::Import(ImportProblem::Changed));
        }
        let (doc, added) =
            commit_import(service.doc(), &current, choice, now).map_err(|error| match error {
                ImportError::ChoiceMismatch => CoreError::Invalid("CHOICE_MISMATCH"),
                ImportError::VaultChanged => CoreError::Import(ImportProblem::Changed),
                ImportError::Edit(_) => CoreError::Save,
            })?;
        service.commit_derived(doc)?;
        Ok(added.len())
    }

    /// Drops a staged import.
    pub fn cancel_staged(&mut self, token: &str) {
        let now = (self.clock)();
        let _ = self.staging.take(token, now);
    }

    fn stage(&mut self, source: Source) -> Result<ImportView, CoreError> {
        let now = (self.clock)();
        let doc = self.preview_doc()?;
        let staged = Staged::new(
            source.candidates.clone(),
            preview(source.candidates, &doc),
            source.skipped,
            now,
        );
        let token = self
            .staging
            .put(staged.clone(), now)
            .ok_or(CoreError::Save)?;
        Ok(view(&token, &staged, &doc, now))
    }

    /// What a preview is made against: the vault, or nothing yet on a first start.
    fn preview_doc(&self) -> Result<VaultDoc, CoreError> {
        match &self.vault {
            Vault::Fresh => Ok(VaultDoc::blank()),
            Vault::Open { service, .. } => Ok(service.doc().clone()),
            Vault::NeedsPassword => Err(CoreError::NeedsPassword),
            Vault::Unavailable(_) => Err(CoreError::Save),
        }
    }

    /// Creates a vault whose key is kept in the system keychain.
    fn create(&mut self) -> Result<(), CoreError> {
        let paths = self.paths.as_ref().ok_or(CoreError::Save)?;
        let open = create_vault(paths, self.keystore.as_ref(), self.lock_wait).map_err(
            |error| match error {
                CreateError::Keystore => CoreError::Import(ImportProblem::NoKeystore),
                _ => CoreError::Save,
            },
        )?;
        self.vault = self.opened(open, false);
        Ok(())
    }
}

fn source_problem(error: SourceError) -> ImportProblem {
    match error {
        SourceError::NeedsPassword => ImportProblem::NeedsBackupPassword,
        SourceError::Backup(BackupError::WrongPassword | BackupError::EmptyPassword) => {
            ImportProblem::WrongBackupPassword
        }
        SourceError::Backup(BackupError::NewerFormat) => ImportProblem::BackupNewer,
        SourceError::Backup(_) => ImportProblem::BackupDamaged,
        SourceError::ImageTooLarge => ImportProblem::ImageTooLarge,
        SourceError::Unknown => ImportProblem::UnknownFormat,
    }
}

fn manual_candidate(form: &ManualForm) -> Result<Candidate, CoreError> {
    let secret = read_typed_secret(&form.secret)?;
    let spec = read_spec(&form.kind, &form.algorithm, form.digits, form.period_s)?;
    if form.issuer.trim().is_empty()
        && form.account.trim().is_empty()
        && form.title.trim().is_empty()
    {
        return Err(CoreError::Invalid("NAME_MISSING"));
    }
    Ok(Candidate {
        title: form.title.trim().to_owned(),
        issuer: form.issuer.trim().to_owned(),
        account: form.account.trim().to_owned(),
        secret,
        spec,
        counter: if form.kind == "hotp" { form.counter } else { 0 },
        note: String::new(),
        tags: Vec::new(),
        origin: Origin::Item(0),
    })
}

#[cfg(test)]
mod tests;

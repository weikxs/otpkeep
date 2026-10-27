//! Leaving the "cannot be opened" state without restarting the app: trying again once another OTPKeep, the
//! keychain or the disk is back, and putting the previous version back when the vault file is damaged.

use keep_vault::store::VaultLock;

use super::{Core, CoreError, PanelState, Problem, Vault};

impl Core {
    /// Tries to open the data again when it could not be opened; any other state stays as it is.
    pub fn retry_open(&mut self) -> PanelState {
        if matches!(self.vault, Vault::Unavailable(_)) {
            self.open();
        }
        self.state("")
    }

    /// Puts the previous version of a damaged vault back and opens it. The damaged file is kept beside it
    /// for diagnosis, so nothing is deleted.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Invalid`] (`NOTHING_TO_RESTORE`) unless the vault is damaged and has a previous
    /// version, `BUSY` when another OTPKeep holds the data, and [`CoreError::Save`] when it cannot be written.
    pub fn restore_previous(&mut self) -> Result<PanelState, CoreError> {
        let restorable =
            matches!(self.vault, Vault::Unavailable(Problem::Damaged)) && self.restorable;
        let Some(paths) = self.paths.as_ref().filter(|_| restorable) else {
            return Err(CoreError::Invalid("NOTHING_TO_RESTORE"));
        };
        {
            // The vault is written only while holding its lock (standards §5).
            let _lock = VaultLock::acquire(paths, self.lock_wait)
                .map_err(|_| CoreError::Invalid("BUSY"))?;
            keep_vault::open::restore_previous(paths).map_err(|_| CoreError::Save)?;
        }
        self.open();
        Ok(self.state(""))
    }
}

#[cfg(test)]
mod tests;

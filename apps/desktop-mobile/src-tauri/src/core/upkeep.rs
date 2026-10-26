//! Housekeeping while the app runs: staged imports (which hold secrets) are dropped once too old, and the
//! trash is emptied of entries older than 30 days once a day, not only when the data opens. Both are cheap
//! checks run from calls the windows make anyway and from one timer per staged import; no timer ticks.

use super::Core;

/// The trash is looked at again after this long.
const TIDY_EVERY_MS: u64 = 24 * 60 * 60 * 1000;

impl Core {
    /// Drops staged imports that are too old and, once a day, empties old entries from the trash.
    pub fn upkeep(&mut self) {
        let now = (self.clock)();
        self.staging.expire(now);
        if self
            .tidied_ms
            .is_none_or(|at| now.saturating_sub(at) >= TIDY_EVERY_MS)
        {
            self.tidy();
        }
    }

    /// Drops every staged import, for when the main window that made them closes.
    pub fn cancel_all_staged(&mut self) {
        self.staging.clear();
    }

    /// How many imports are staged, for tests.
    #[cfg(test)]
    pub(crate) const fn staged_count(&self) -> usize {
        self.staging.count()
    }
}

#[cfg(test)]
mod tests;

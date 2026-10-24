//! Imports waiting for the person to choose what to add (PRD FR-6.6).
//!
//! The candidates, with their secrets, stay here in the core; the window only gets a token and display
//! fields. A staged import is dropped when it is committed, cancelled, replaced by newer ones or too old.

use keep_gate::session::Token;
use keep_transfer::import::{Candidate, ImportPreview};
use keep_transfer::source::Skipped;

/// A staged import is forgotten after this long.
pub const STAGED_TTL_MS: u64 = 10 * 60 * 1000;
/// At most this many imports wait at once; the oldest goes first.
pub const MAX_STAGED: usize = 4;

/// One import waiting for a choice.
#[derive(Debug, Clone)]
pub struct Staged {
    /// What was read, in source order; kept so the preview can be made again against the current vault.
    pub candidates: Vec<Candidate>,
    /// The preview the person sees.
    pub preview: ImportPreview,
    /// What could not be read.
    pub skipped: Vec<Skipped>,
    at_ms: u64,
}

impl Staged {
    #[must_use]
    pub fn new(
        candidates: Vec<Candidate>,
        preview: ImportPreview,
        skipped: Vec<Skipped>,
        now_ms: u64,
    ) -> Self {
        Self {
            candidates,
            preview,
            skipped,
            at_ms: now_ms,
        }
    }
}

/// The staged imports, by token.
#[derive(Debug, Default)]
pub struct Staging {
    items: Vec<(String, Staged)>,
}

impl Staging {
    /// Keeps `staged` and returns its token, or `None` when no random token can be made.
    pub fn put(&mut self, staged: Staged, now_ms: u64) -> Option<String> {
        self.expire(now_ms);
        let token = Token::random().ok()?.as_str().to_owned();
        if self.items.len() >= MAX_STAGED {
            self.items.remove(0);
        }
        self.items.push((token.clone(), staged));
        Some(token)
    }

    /// Puts an import back under the token it had, for example after previewing it again.
    pub fn put_back(&mut self, token: &str, staged: Staged) -> bool {
        if self.items.iter().any(|(t, _)| t == token) {
            return false;
        }
        if self.items.len() >= MAX_STAGED {
            self.items.remove(0);
        }
        self.items.push((token.to_owned(), staged));
        true
    }

    /// The import under `token`, if it is still waiting.
    pub fn get(&mut self, token: &str, now_ms: u64) -> Option<&Staged> {
        self.expire(now_ms);
        self.items.iter().find(|(t, _)| t == token).map(|(_, s)| s)
    }

    /// Takes the import under `token` out.
    pub fn take(&mut self, token: &str, now_ms: u64) -> Option<Staged> {
        self.expire(now_ms);
        let at = self.items.iter().position(|(t, _)| t == token)?;
        Some(self.items.remove(at).1)
    }

    fn expire(&mut self, now_ms: u64) {
        self.items
            .retain(|(_, s)| now_ms.saturating_sub(s.at_ms) <= STAGED_TTL_MS);
    }
}

#[cfg(test)]
mod tests {
    use keep_transfer::import::preview;
    use keep_vault::VaultDoc;

    use super::{MAX_STAGED, STAGED_TTL_MS, Staged, Staging};

    const T0: u64 = 1_000_000;

    fn staged(now: u64) -> Staged {
        Staged::new(
            Vec::new(),
            preview(Vec::new(), &VaultDoc::blank()),
            Vec::new(),
            now,
        )
    }

    #[test]
    fn keeps_an_import_under_a_random_token_until_taken() {
        let mut staging = Staging::default();
        let token = staging.put(staged(T0), T0).unwrap();

        assert_eq!(token.len(), 64);
        assert!(staging.get(&token, T0).is_some());
        assert!(staging.take(&token, T0).is_some());
        assert!(staging.take(&token, T0).is_none());
    }

    #[test]
    fn forgets_old_imports() {
        let mut staging = Staging::default();
        let token = staging.put(staged(T0), T0).unwrap();

        assert!(staging.get(&token, T0 + STAGED_TTL_MS + 1).is_none());
    }

    #[test]
    fn keeps_only_the_newest_few() {
        let mut staging = Staging::default();
        let first = staging.put(staged(T0), T0).unwrap();
        for _ in 0..MAX_STAGED {
            staging.put(staged(T0), T0).unwrap();
        }

        assert!(staging.get(&first, T0).is_none());
    }

    #[test]
    fn puts_an_import_back_under_its_token_once() {
        let mut staging = Staging::default();
        let token = staging.put(staged(T0), T0).unwrap();
        let removed = staging.take(&token, T0).unwrap();

        assert!(staging.put_back(&token, removed));
        assert!(!staging.put_back(&token, staged(T0)));
        assert!(staging.get(&token, T0).is_some());
    }
}

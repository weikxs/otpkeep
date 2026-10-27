//! Choosing the rows to show, with the search index kept between keystrokes (PRD FR-3.10).
//!
//! The index is built on the first search after the vault changed and reused until it changes again. Every
//! change goes through [`VaultService`], which counts them in its revision, so a save, an import, a sync merge or
//! a new vault after unlocking can never leave an old index in use. The index holds display fields only.

use std::cell::RefCell;

use keep_find::{FindContext, FindIndex};
use keep_vault::service::VaultService;
use keep_vault::{Entry, EntryId};

use super::SEARCH_LIMIT;

/// The search index of one open vault and the revision it was built from.
#[derive(Default)]
pub(super) struct FindCache(RefCell<Option<(u64, FindIndex)>>);

impl FindCache {
    /// The best matches for `query` among `entries`, which must be `service`'s entries in list order.
    fn find(&self, service: &VaultService, entries: &[&Entry], query: &str) -> Vec<EntryId> {
        let revision = service.revision();
        let mut cached = self.0.borrow_mut();
        if cached.as_ref().is_none_or(|(built, _)| *built != revision) {
            *cached = Some((revision, FindIndex::build(entries)));
        }
        let Some((_, index)) = cached.as_ref() else {
            return Vec::new();
        };
        index
            .find(query, &FindContext::default())
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|hit| hit.entry)
            .collect()
    }

    /// The revision the index was last built from, if it was built.
    #[cfg(test)]
    pub(super) fn built_from(&self) -> Option<u64> {
        self.0.borrow().as_ref().map(|(revision, _)| *revision)
    }
}

/// Without a search, `ordered` (the entries in list order) up to `limit`; with one, the best matches.
pub(super) fn select<'a>(
    service: &'a VaultService,
    cache: &FindCache,
    ordered: &[&'a Entry],
    query: &str,
    limit: Option<usize>,
) -> Vec<&'a Entry> {
    if query.trim().is_empty() {
        return ordered
            .iter()
            .take(limit.unwrap_or(usize::MAX))
            .copied()
            .collect();
    }
    cache
        .find(service, ordered, query)
        .into_iter()
        .filter_map(|id| service.entry(id))
        .collect()
}

#[cfg(test)]
mod tests;

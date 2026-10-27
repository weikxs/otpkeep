//! Search over names, issuers, accounts, tags and notes, including pinyin (design/04-search.md).
//!
//! The index lives in memory only and is rebuilt from the entries; nothing about searches is stored.

use std::collections::HashSet;
use std::ops::Range;

use keep_vault::{Entry, EntryId};

mod fold;
mod matcher;
mod syllable;

use matcher::{Prepared, best_match, merge};
use syllable::Readings;

/// Score added for an entry used in the last 7 days.
const RECENT_BONUS: u32 = 100;
/// Score added for a pinned entry.
const PINNED_BONUS: u32 = 50;
/// Characters shown on each side of a note match.
const SNIPPET_CONTEXT_CHARS: usize = 20;
const ELLIPSIS: char = '…';

/// Which field a match is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// Display name.
    Title,
    /// Service name.
    Issuer,
    /// Account name.
    Account,
    /// Tags, joined with single spaces; highlight positions refer to that joined text.
    Tags,
    /// Note.
    Note,
}

impl FieldKind {
    /// How much a match in this field counts, in tenths.
    const fn weight(self) -> u32 {
        match self {
            Self::Title => 10,
            Self::Issuer => 9,
            Self::Account | Self::Tags => 8,
            Self::Note => 6,
        }
    }
}

/// What the ranking may use besides the text.
#[derive(Debug, Clone, Default)]
pub struct FindContext {
    /// Entries used in the last 7 days.
    pub recently_used: HashSet<EntryId>,
}

/// One result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindHit {
    /// The entry.
    pub entry: EntryId,
    /// Higher is better.
    pub score: u32,
    /// Matched original-character ranges per field.
    pub matched: Vec<(FieldKind, Vec<Range<usize>>)>,
    /// When the note matched: the text around the first match.
    pub note_snippet: Option<String>,
}

struct Record {
    entry: EntryId,
    pinned: bool,
    note: String,
    fields: Vec<(FieldKind, Prepared)>,
}

/// The searchable form of a list of entries.
pub struct FindIndex {
    records: Vec<Record>,
}

impl FindIndex {
    /// Prepares `entries`; results with equal scores keep this order.
    #[must_use]
    pub fn build(entries: &[&Entry]) -> Self {
        let mut readings = Readings::default();
        let records = entries
            .iter()
            .map(|e| Record {
                entry: e.id,
                pinned: e.pinned.value,
                note: e.note.value.clone(),
                fields: vec![
                    (
                        FieldKind::Title,
                        Prepared::new(&e.title.value, &mut readings),
                    ),
                    (
                        FieldKind::Issuer,
                        Prepared::new(&e.issuer.value, &mut readings),
                    ),
                    (
                        FieldKind::Account,
                        Prepared::new(&e.account.value, &mut readings),
                    ),
                    (
                        FieldKind::Tags,
                        Prepared::new(&e.tags.value.join(" "), &mut readings),
                    ),
                    (FieldKind::Note, Prepared::new(&e.note.value, &mut readings)),
                ],
            })
            .collect();
        Self { records }
    }

    /// Entries matching every word of `query`, best first. An empty query returns every entry in order.
    #[must_use]
    pub fn find(&self, query: &str, context: &FindContext) -> Vec<FindHit> {
        let folded: Vec<char> = fold::fold(query).chars;
        let terms: Vec<&[char]> = folded
            .split(|c| *c == ' ')
            .filter(|t| !t.is_empty())
            .collect();
        let mut hits: Vec<(usize, FindHit)> = self
            .records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| record.hit(&terms, context).map(|hit| (index, hit)))
            .collect();
        hits.sort_by(|(ia, a), (ib, b)| b.score.cmp(&a.score).then(ia.cmp(ib)));
        hits.into_iter().map(|(_, hit)| hit).collect()
    }
}

impl Record {
    fn hit(&self, terms: &[&[char]], context: &FindContext) -> Option<FindHit> {
        let mut score = 0;
        let mut matched: Vec<(FieldKind, Vec<Range<usize>>)> = Vec::new();
        for term in terms {
            let (term_score, kind, ranges) = self
                .fields
                .iter()
                .filter_map(|(kind, field)| {
                    best_match(field, term).map(|(base, r)| (base * kind.weight(), *kind, r))
                })
                .max_by_key(|(s, kind, _)| (*s, std::cmp::Reverse(*kind as u8)))?;
            score += term_score;
            match matched.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, existing)) => *existing = merge([existing.clone(), ranges].concat()),
                None => matched.push((kind, ranges)),
            }
        }
        if context.recently_used.contains(&self.entry) {
            score += RECENT_BONUS;
        }
        if self.pinned {
            score += PINNED_BONUS;
        }
        let note_snippet = matched
            .iter()
            .find(|(k, _)| *k == FieldKind::Note)
            .and_then(|(_, ranges)| ranges.first())
            .map(|first| snippet(&self.note, first));
        Some(FindHit {
            entry: self.entry,
            score,
            matched,
            note_snippet,
        })
    }
}

/// The note around `range`, on one line, with an ellipsis where text was cut.
fn snippet(note: &str, range: &Range<usize>) -> String {
    let chars: Vec<char> = note
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let from = range.start.saturating_sub(SNIPPET_CONTEXT_CHARS);
    let to = (range.end + SNIPPET_CONTEXT_CHARS).min(chars.len());
    let mut out = String::new();
    if from > 0 {
        out.push(ELLIPSIS);
    }
    out.extend(&chars[from..to]);
    if to < chars.len() {
        out.push(ELLIPSIS);
    }
    out
}

#[cfg(test)]
mod tests;

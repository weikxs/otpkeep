//! Matching one search term against one field (design/04-search.md §3).
//!
//! Matches return lists of highlight ranges; a list with a single range is intended, not `0..n` by mistake.
#![allow(clippy::single_range_in_vec_init)]

use std::ops::Range;

use crate::fold::{Folded, fold};
use crate::syllable::{Unit, units};

/// Base scores by kind of match, strongest first.
pub(crate) const EXACT: u32 = 100;
pub(crate) const PREFIX: u32 = 80;
pub(crate) const WORD_PREFIX: u32 = 70;
pub(crate) const PINYIN_INITIALS_PREFIX: u32 = 65;
pub(crate) const PINYIN_FULL_PREFIX: u32 = 60;
pub(crate) const SUBSTRING: u32 = 50;
pub(crate) const PINYIN_INSIDE: u32 = 45;
pub(crate) const FUZZY: u32 = 30;
const FUZZY_MIN: u32 = 10;

/// A field prepared for matching.
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub(crate) folded: Folded,
    pub(crate) units: Vec<Unit>,
}

impl Prepared {
    pub(crate) fn new(text: &str) -> Self {
        Self {
            folded: fold(text),
            units: units(text),
        }
    }
}

/// The best match of `term` (already folded, non-empty, without spaces) in `field`: base score and the matched
/// original-character ranges.
pub(crate) fn best_match(field: &Prepared, term: &[char]) -> Option<(u32, Vec<Range<usize>>)> {
    let text = &field.folded.chars;
    if text.is_empty() || term.is_empty() {
        return None;
    }
    if text.as_slice() == term {
        return Some((EXACT, span(&field.folded, 0, text.len())));
    }
    if text.starts_with(term) {
        return Some((PREFIX, span(&field.folded, 0, term.len())));
    }
    if let Some(at) =
        (1..text.len()).find(|&i| !text[i - 1].is_alphanumeric() && text[i..].starts_with(term))
    {
        return Some((WORD_PREFIX, span(&field.folded, at, at + term.len())));
    }
    let pinyin = pinyin_match(&field.units, term);
    if let Some((score, ranges)) = pinyin.clone().filter(|(score, _)| *score > SUBSTRING) {
        return Some((score, ranges));
    }
    if let Some(at) = text.windows(term.len()).position(|w| w == term) {
        return Some((SUBSTRING, span(&field.folded, at, at + term.len())));
    }
    pinyin.or_else(|| fuzzy(&field.folded, term))
}

/// Pinyin initials or full spelling, starting at any unit; the span must contain a Chinese character.
fn pinyin_match(units: &[Unit], term: &[char]) -> Option<(u32, Vec<Range<usize>>)> {
    let mut best: Option<(u32, Vec<Range<usize>>)> = None;
    for start in 0..units.len() {
        let found = [
            (initials(units, start, term), PINYIN_INITIALS_PREFIX),
            (full(units, start, term), PINYIN_FULL_PREFIX),
        ];
        for (end, prefix_score) in found {
            let Some(end) = end else { continue };
            if !units[start..=end].iter().any(|u| u.is_han) {
                continue;
            }
            let score = if start == 0 {
                prefix_score
            } else {
                PINYIN_INSIDE
            };
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, vec![units[start].origin..units[end].origin + 1]));
            }
        }
    }
    best
}

/// Each term character is the first letter of one reading of consecutive units; returns the last unit used.
fn initials(units: &[Unit], start: usize, term: &[char]) -> Option<usize> {
    let end = start + term.len() - 1;
    let fits = units
        .get(start..=end)?
        .iter()
        .zip(term)
        .all(|(unit, ch)| unit.readings.iter().any(|r| r.first() == Some(ch)));
    fits.then_some(end)
}

/// The term spells whole readings of consecutive units; the last may be spelled only partly.
fn full(units: &[Unit], at: usize, term: &[char]) -> Option<usize> {
    let unit = units.get(at)?;
    for reading in &unit.readings {
        if reading.starts_with(term) {
            return Some(at);
        }
        if term.starts_with(reading) {
            let rest = &term[reading.len()..];
            if rest.is_empty() {
                return Some(at);
            }
            if let Some(end) = full(units, at + 1, rest) {
                return Some(end);
            }
        }
    }
    None
}

/// The term's characters in order, with gaps; fewer gaps score higher.
fn fuzzy(folded: &Folded, term: &[char]) -> Option<(u32, Vec<Range<usize>>)> {
    if term.len() < 2 {
        return None;
    }
    let mut positions = Vec::with_capacity(term.len());
    let mut from = 0;
    for ch in term {
        let at = from + folded.chars[from..].iter().position(|c| c == ch)?;
        positions.push(at);
        from = at + 1;
    }
    let gaps = positions.last().copied().unwrap_or(0) - positions.first().copied().unwrap_or(0) + 1
        - term.len();
    let score = FUZZY
        .saturating_sub(u32::try_from(gaps).unwrap_or(u32::MAX))
        .max(FUZZY_MIN);
    let ranges = positions
        .iter()
        .map(|&p| folded.origin[p]..folded.origin[p] + 1)
        .collect();
    Some((score, merge(ranges)))
}

/// Original-character range covered by folded characters `from..to`.
fn span(folded: &Folded, from: usize, to: usize) -> Vec<Range<usize>> {
    vec![folded.origin[from]..folded.origin[to - 1] + 1]
}

/// Sorts ranges and joins overlapping or touching ones.
pub(crate) fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

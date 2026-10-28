//! Folding text for comparison: compatibility normalization (full-width to half-width), lower case, accents
//! removed and runs of whitespace collapsed to one space. Every folded character remembers which original
//! character it came from, so matches can be highlighted in the original text.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Folded text and, for each folded character, the index of the original character it came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folded {
    pub(crate) chars: Vec<char>,
    pub(crate) origin: Vec<usize>,
}

/// Folds `text`.
pub(crate) fn fold(text: &str) -> Folded {
    let mut folded = Folded {
        chars: Vec::with_capacity(text.len()),
        origin: Vec::with_capacity(text.len()),
    };
    for (index, original) in text.chars().enumerate() {
        if original.is_ascii() || is_unified_ideograph(original) {
            push(&mut folded, original.to_ascii_lowercase(), index);
            continue;
        }
        for ch in fold_char(original) {
            push(&mut folded, ch, index);
        }
    }
    if folded.chars.last() == Some(&' ') {
        folded.chars.pop();
        folded.origin.pop();
    }
    folded
}

/// Adds one folded character, collapsing runs of whitespace and dropping leading whitespace.
fn push(folded: &mut Folded, ch: char, index: usize) {
    if ch.is_whitespace() {
        if folded.chars.last().is_some_and(|last| *last != ' ') {
            folded.chars.push(' ');
            folded.origin.push(index);
        }
    } else {
        folded.chars.push(ch);
        folded.origin.push(index);
    }
}

/// The folded characters of one original character.
pub(crate) fn fold_char(original: char) -> Vec<char> {
    // ASCII is already in normal form and has no marks; skipping normalization keeps building the index fast.
    if original.is_ascii() {
        return vec![original.to_ascii_lowercase()];
    }
    if is_unified_ideograph(original) {
        return vec![original];
    }
    normalized(original)
}

/// A Chinese character of the CJK Unified Ideographs block or its Extension A. These have no compatibility or
/// canonical decomposition and no case, so they fold to themselves; names and notes are full of them, so
/// skipping normalization for them keeps building the index fast.
fn is_unified_ideograph(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}')
}

/// [`fold_char`] the long way: compatibility normalization, marks removed, lower case.
fn normalized(original: char) -> Vec<char> {
    std::iter::once(original)
        .nfkc()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Folds a string without positions (for pinyin readings and queries).
pub(crate) fn fold_plain(text: &str) -> String {
    fold(text).chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{fold, fold_char, normalized};

    #[test]
    fn the_ascii_shortcut_folds_like_the_long_way() {
        for ch in (0u8..128).map(char::from) {
            assert_eq!(fold_char(ch), normalized(ch), "{ch:?}");
        }
    }

    #[test]
    fn the_chinese_shortcut_folds_like_the_long_way() {
        for ch in ('\u{3400}'..='\u{4DBF}').chain('\u{4E00}'..='\u{9FFF}') {
            assert_eq!(fold_char(ch), normalized(ch), "{ch:?}");
            assert_eq!(fold(&ch.to_string()).chars, normalized(ch), "{ch:?}");
        }
    }
}

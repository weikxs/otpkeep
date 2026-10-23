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
    let mut folded = Folded::default();
    for (index, original) in text.chars().enumerate() {
        for ch in fold_char(original) {
            if ch.is_whitespace() {
                // Collapse runs of whitespace and drop leading whitespace.
                if folded.chars.last().is_some_and(|last| *last != ' ') {
                    folded.chars.push(' ');
                    folded.origin.push(index);
                }
            } else {
                folded.chars.push(ch);
                folded.origin.push(index);
            }
        }
    }
    if folded.chars.last() == Some(&' ') {
        folded.chars.pop();
        folded.origin.pop();
    }
    folded
}

/// The folded characters of one original character.
pub(crate) fn fold_char(original: char) -> Vec<char> {
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

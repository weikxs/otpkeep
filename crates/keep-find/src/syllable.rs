//! Text as a sequence of syllable units for pinyin matching. A Chinese character is one unit with all of its
//! readings; any other character is one unit per folded character, read as itself. Whitespace is skipped.

use std::collections::HashMap;
use std::sync::Arc;

use pinyin::ToPinyinMulti;

use crate::fold::{fold_char, fold_plain};

/// One unit: where it came from, its readings, and whether it is a Chinese character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unit {
    pub(crate) origin: usize,
    pub(crate) readings: Arc<[Vec<char>]>,
    pub(crate) is_han: bool,
}

/// The readings of the characters seen so far, shared by every unit of the same character. Names and notes
/// repeat characters a lot, so building an index looks each one up once.
#[derive(Debug, Default)]
pub(crate) struct Readings {
    han: HashMap<char, Option<Arc<[Vec<char>]>>>,
    plain: HashMap<char, Arc<[Vec<char>]>>,
}

impl Readings {
    fn han(&mut self, ch: char) -> Option<Arc<[Vec<char>]>> {
        if ch.is_ascii() {
            return None;
        }
        self.han
            .entry(ch)
            .or_insert_with(|| han_readings(ch).map(Arc::from))
            .clone()
    }

    /// A character that is not Chinese reads as itself.
    fn plain(&mut self, folded: char) -> Arc<[Vec<char>]> {
        Arc::clone(
            self.plain
                .entry(folded)
                .or_insert_with(|| Arc::from(vec![vec![folded]])),
        )
    }
}

/// The units of `text`.
pub(crate) fn units(text: &str, readings: &mut Readings) -> Vec<Unit> {
    let mut units = Vec::with_capacity(text.len());
    for (origin, ch) in text.chars().enumerate() {
        if let Some(han) = readings.han(ch) {
            units.push(Unit {
                origin,
                readings: han,
                is_han: true,
            });
            continue;
        }
        let folded = if ch.is_ascii() {
            vec![ch.to_ascii_lowercase()]
        } else {
            fold_char(ch)
        };
        for folded in folded.into_iter().filter(|c| !c.is_whitespace()) {
            units.push(Unit {
                origin,
                readings: readings.plain(folded),
                is_han: false,
            });
        }
    }
    units
}

/// Every distinct toneless reading of a Chinese character, folded like a query. A reading with `ü` is also
/// offered with `v`, the way most keyboards type it (folding already turns `ü` into `u`).
fn han_readings(ch: char) -> Option<Vec<Vec<char>>> {
    let multi = ch.to_pinyin_multi()?;
    let mut readings: Vec<Vec<char>> = Vec::new();
    for pinyin in multi {
        let plain = pinyin.plain();
        let mut spellings = vec![fold_plain(plain)];
        if plain.contains('ü') {
            spellings.push(plain.replace('ü', "v"));
        }
        for spelling in spellings {
            let chars: Vec<char> = spelling.chars().collect();
            if !chars.is_empty() && !readings.contains(&chars) {
                readings.push(chars);
            }
        }
    }
    (!readings.is_empty()).then_some(readings)
}

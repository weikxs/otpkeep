//! Text as a sequence of syllable units for pinyin matching. A Chinese character is one unit with all of its
//! readings; any other character is one unit per folded character, read as itself. Whitespace is skipped.

use pinyin::ToPinyinMulti;

use crate::fold::{fold_char, fold_plain};

/// One unit: where it came from, its readings, and whether it is a Chinese character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unit {
    pub(crate) origin: usize,
    pub(crate) readings: Vec<Vec<char>>,
    pub(crate) is_han: bool,
}

/// The units of `text`.
pub(crate) fn units(text: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    for (origin, ch) in text.chars().enumerate() {
        if let Some(readings) = han_readings(ch) {
            units.push(Unit {
                origin,
                readings,
                is_han: true,
            });
            continue;
        }
        for folded in fold_char(ch).into_iter().filter(|c| !c.is_whitespace()) {
            units.push(Unit {
                origin,
                readings: vec![vec![folded]],
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

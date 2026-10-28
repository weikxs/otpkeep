//! Text as a sequence of syllable units for pinyin matching. A Chinese character is one unit with all of its
//! readings; any other character is one unit per folded character, read as itself. Whitespace is skipped.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
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
    han: HashMap<char, Option<Arc<[Vec<char>]>>, CharHash>,
    /// Indexed by ASCII code; most characters of names and accounts are ASCII.
    ascii: Vec<Option<Arc<[Vec<char>]>>>,
    plain: HashMap<char, Arc<[Vec<char>]>, CharHash>,
}

/// Hashes a `char` key with one multiplication. The keys are characters of the user's own entries, and the
/// lookups are per character of every field, where the default hasher's cost showed in building the index.
type CharHash = BuildHasherDefault<CharHasher>;

#[derive(Debug, Default)]
struct CharHasher(u64);

impl CharHasher {
    /// An odd constant with well-mixed bits (the one of rustc's `FxHasher`).
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    fn mix(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(Self::SEED);
    }
}

impl Hasher for CharHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.mix(u64::from(*byte));
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.mix(u64::from(value));
    }
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
        if let Ok(code) = u8::try_from(folded)
            && code.is_ascii()
        {
            if self.ascii.is_empty() {
                self.ascii.resize(128, None);
            }
            let slot = &mut self.ascii[usize::from(code)];
            return Arc::clone(slot.get_or_insert_with(|| Arc::from(vec![vec![folded]])));
        }
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
        if ch.is_ascii() {
            if !ch.is_whitespace() {
                units.push(Unit {
                    origin,
                    readings: readings.plain(ch.to_ascii_lowercase()),
                    is_han: false,
                });
            }
            continue;
        }
        for folded in fold_char(ch).into_iter().filter(|c| !c.is_whitespace()) {
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

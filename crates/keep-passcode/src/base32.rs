//! Base32 (RFC 4648) for two-factor secrets.
//!
//! Decoding is lenient about how people write secrets: any case, spaces and dashes between groups, and any
//! amount of `=` padding (or none). Encoding is canonical: upper case without padding.

use zeroize::Zeroizing;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BITS_PER_SYMBOL: u32 = 5;
const BITS_PER_BYTE: u32 = 8;

/// Why a text is not a Base32 secret. Positions count characters (not bytes) of the original text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Base32Fault {
    /// A character outside the Base32 alphabet.
    #[error("'{found}' at position {at} is not a Base32 character")]
    InvalidChar {
        /// Character position in the original text.
        at: usize,
        /// The offending character.
        found: char,
    },
    /// `=` padding followed by more data.
    #[error("padding at position {at} is followed by more characters")]
    MisplacedPadding {
        /// Position of the first `=`.
        at: usize,
    },
    /// The text holds no whole byte of data.
    #[error("the secret is empty")]
    Empty,
}

/// Decodes a Base32 secret as people write it.
///
/// # Errors
///
/// Returns [`Base32Fault`] with the position of the first problem.
pub fn decode(text: &str) -> Result<Zeroizing<Vec<u8>>, Base32Fault> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let mut buffer: u32 = 0;
    let mut buffered_bits: u32 = 0;
    let mut padding_at = None;

    for (at, found) in text.chars().enumerate() {
        if found.is_whitespace() || found == '-' {
            continue;
        }
        if found == '=' {
            padding_at.get_or_insert(at);
            continue;
        }
        if let Some(at) = padding_at {
            return Err(Base32Fault::MisplacedPadding { at });
        }
        let value = symbol_value(found).ok_or(Base32Fault::InvalidChar { at, found })?;
        buffer = (buffer << BITS_PER_SYMBOL) | value;
        buffered_bits += BITS_PER_SYMBOL;
        if buffered_bits >= BITS_PER_BYTE {
            buffered_bits -= BITS_PER_BYTE;
            // Only the low eight bits are taken, so the truncation is intended.
            #[allow(clippy::cast_possible_truncation)]
            bytes.push((buffer >> buffered_bits) as u8);
            buffer &= (1 << buffered_bits) - 1;
        }
    }
    if bytes.is_empty() {
        return Err(Base32Fault::Empty);
    }
    Ok(bytes)
}

/// Encodes bytes as canonical Base32: upper case, no padding.
#[must_use]
pub fn encode(bytes: &[u8]) -> Zeroizing<String> {
    let mut text = Zeroizing::new(String::with_capacity(bytes.len().div_ceil(5) * 8));
    let mut buffer: u32 = 0;
    let mut buffered_bits: u32 = 0;

    for &byte in bytes {
        buffer = (buffer << BITS_PER_BYTE) | u32::from(byte);
        buffered_bits += BITS_PER_BYTE;
        while buffered_bits >= BITS_PER_SYMBOL {
            buffered_bits -= BITS_PER_SYMBOL;
            text.push(symbol((buffer >> buffered_bits) & 0x1f));
        }
        buffer &= (1 << buffered_bits) - 1;
    }
    if buffered_bits > 0 {
        text.push(symbol((buffer << (BITS_PER_SYMBOL - buffered_bits)) & 0x1f));
    }
    text
}

fn symbol_value(ch: char) -> Option<u32> {
    match ch.to_ascii_uppercase() {
        upper @ 'A'..='Z' => Some(u32::from(upper) - u32::from('A')),
        digit @ '2'..='7' => Some(u32::from(digit) - u32::from('2') + 26),
        _ => None,
    }
}

fn symbol(value: u32) -> char {
    // `value` is masked to five bits by every caller, so it always indexes the alphabet.
    usize::try_from(value)
        .ok()
        .and_then(|index| ALPHABET.get(index))
        .map_or('A', |&b| char::from(b))
}

#[cfg(test)]
mod tests;

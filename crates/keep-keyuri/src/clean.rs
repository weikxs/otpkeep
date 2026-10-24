//! Getting the link out of what people paste (design/05-transfer.md; behaviour compared with the Aegis and FreeOTP apps).
//!
//! Text copied from chats, mails and web pages brings invisible characters, words around the link and quotes
//! or brackets; a Chinese input method may type letters and digits in full width. Names are left exactly as
//! written: only the parts a machine reads (the secret and the numbers) are folded to ordinary width.

use keep_passcode::SHORT_SECRET_BYTES;
use keep_passcode::base32;
use zeroize::Zeroizing;

use crate::KeyUriFault;

const LINK_START: &str = "otpauth://";
/// Characters copying adds that are never part of a link.
const INVISIBLE: [char; 6] = [
    '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{ad}',
];
/// Quotes and brackets a link is often wrapped in, with their closing partner.
const WRAPPERS: [(char, char); 12] = [
    ('"', '"'),
    ('\'', '\''),
    ('“', '”'),
    ('‘', '’'),
    ('<', '>'),
    ('(', ')'),
    ('（', '）'),
    ('[', ']'),
    ('【', '】'),
    ('「', '」'),
    ('『', '』'),
    ('《', '》'),
];
/// A Chinese full stop after a link ends the sentence, not the link.
const SENTENCE_END: char = '。';
/// Other apps' codes that are not `otpauth://` links, and what to tell about each.
const FOREIGN: [(&str, KeyUriFault); 3] = [
    ("otpauth-migration://", KeyUriFault::GoogleExport),
    ("phonefactor://", KeyUriFault::MicrosoftPush),
    ("oraclemobileauthenticator://", KeyUriFault::OracleEnrolment),
];

/// The text without invisible characters. It may hold a secret, so it is wiped when dropped.
#[must_use]
pub(crate) fn without_invisible(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.chars().filter(|c| !INVISIBLE.contains(c)).collect())
}

/// The link in `text`: from `otpauth://` to the end of its line, without a wrapping quote or bracket.
/// Text without such a link is returned trimmed, unless it is recognisably something else.
///
/// # Errors
///
/// Returns the [`KeyUriFault`] that names what the text is instead: another app's code or a bare secret.
pub(crate) fn locate(text: &str) -> Result<&str, KeyUriFault> {
    let lower = text.to_ascii_lowercase();
    let Some(start) = lower.find(LINK_START) else {
        classify(text.trim(), &lower)?;
        return Ok(text.trim());
    };
    let line = &text[start..];
    let line = line
        .find(['\r', '\n'])
        .map_or(line, |end| &line[..end])
        .trim_end();
    let line = line.strip_suffix(SENTENCE_END).unwrap_or(line);
    let opener = text[..start].chars().next_back();
    let closer = WRAPPERS
        .iter()
        .find(|(open, _)| Some(*open) == opener)
        .map(|(_, close)| *close);
    Ok(closer
        .and_then(|close| line.strip_suffix(close))
        .unwrap_or(line))
}

/// Names text that is not an `otpauth://` link but something recognisable; `Ok` when it is not.
fn classify(trimmed: &str, lower: &str) -> Result<(), KeyUriFault> {
    if let Some((_, fault)) = FOREIGN.iter().find(|(scheme, _)| lower.contains(scheme)) {
        return Err(fault.clone());
    }
    if !trimmed.contains("://") && looks_like_secret(trimmed) {
        return Err(KeyUriFault::BareSecret);
    }
    Ok(())
}

/// Base32 of a usable length, with at least one of the digits Base32 uses: ordinary words have none.
fn looks_like_secret(text: &str) -> bool {
    let folded = fold_width(text);
    let only_base32 = folded
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || matches!(c, '-' | '='));
    only_base32
        && folded.chars().any(|c| ('2'..='7').contains(&c))
        && base32::decode(&folded).is_ok_and(|bytes| bytes.len() >= SHORT_SECRET_BYTES)
}

/// Full-width letters, digits and signs (as a Chinese input method types them) in ordinary width. The value
/// may be a secret, so the copy is wiped when dropped.
#[must_use]
pub(crate) fn fold_width(text: &str) -> Zeroizing<String> {
    const FULL_WIDTH: std::ops::RangeInclusive<u32> = 0xFF01..=0xFF5E;
    const OFFSET: u32 = 0xFEE0;
    const IDEOGRAPHIC_SPACE: char = '\u{3000}';
    Zeroizing::new(
        text.chars()
            .map(|c| match u32::from(c) {
                code if FULL_WIDTH.contains(&code) => char::from_u32(code - OFFSET).unwrap_or(c),
                _ if c == IDEOGRAPHIC_SPACE => ' ',
                _ => c,
            })
            .collect(),
    )
}

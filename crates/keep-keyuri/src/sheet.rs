//! Link sheets: plain-text lists of `otpauth://` links, one per line.
//!
//! OTPKeep writes each entry as an optional `# title` line, an optional `# note: …` line and the link, with a
//! blank line between entries. Comment lines are expected to be ignored by other apps, so the sheet stays a
//! plain list of links for them. Reading is forgiving: a bad line is reported with its number and skipped.

use std::fmt::Write as _;

use zeroize::Zeroizing;

use crate::{KeyUri, KeyUriFault, KeyUriNote, parse_key_uri, write_key_uri};

const NOTE_PREFIX: &str = "note:";

/// One entry to write.
#[derive(Debug, Clone, Copy)]
pub struct SheetEntry<'a> {
    /// Display name; written on one line, or left out when empty.
    pub title: &'a str,
    /// Note; line breaks are escaped, left out when empty.
    pub note: &'a str,
    /// The account itself.
    pub uri: &'a KeyUri,
}

/// One entry read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetItem {
    /// The `# title` line before the link, if any.
    pub title: Option<String>,
    /// The `# note:` line before the link, if any.
    pub note: Option<String>,
    /// The parsed link.
    pub uri: KeyUri,
    /// Observations about the link, such as a weak secret.
    pub notes: Vec<KeyUriNote>,
}

/// A line that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetProblem {
    /// Line number, starting at 1.
    pub line: usize,
    /// What is wrong with it.
    pub kind: SheetProblemKind,
}

/// What is wrong with a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetProblemKind {
    /// Neither a comment nor a link.
    NotALink,
    /// A link that does not parse.
    BadLink(KeyUriFault),
}

/// Everything read from a sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetRead {
    /// The entries, in order.
    pub items: Vec<SheetItem>,
    /// Lines that were skipped.
    pub problems: Vec<SheetProblem>,
}

/// Writes a sheet. It contains secrets, so it is wiped from memory when dropped.
#[must_use]
pub fn write_sheet(entries: &[SheetEntry<'_>]) -> Zeroizing<String> {
    let mut text = Zeroizing::new(String::new());
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        let title = entry.title.split_whitespace().collect::<Vec<_>>().join(" ");
        if !title.is_empty() {
            let _ = writeln!(text, "# {title}"); // writing to a String cannot fail
        }
        if !entry.note.is_empty() {
            let _ = writeln!(text, "# {NOTE_PREFIX} {}", escape(entry.note));
        }
        text.push_str(&write_key_uri(entry.uri));
        text.push('\n');
    }
    text
}

/// Reads a sheet. A title or note applies to the next link only.
#[must_use]
pub fn read_sheet(text: &str) -> SheetRead {
    let mut read = SheetRead::default();
    let mut title = None;
    let mut note = None;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            let comment = comment.trim();
            match comment.strip_prefix(NOTE_PREFIX) {
                Some(body) => note = Some(unescape(body.trim_start())),
                None if !comment.is_empty() => title = Some(comment.to_owned()),
                None => {}
            }
            continue;
        }
        let (title, note) = (title.take(), note.take());
        let kind = if line.contains("://") {
            match parse_key_uri(line) {
                Ok(parsed) => {
                    read.items.push(SheetItem {
                        title,
                        note,
                        uri: parsed.uri,
                        notes: parsed.notes,
                    });
                    continue;
                }
                Err(fault) => SheetProblemKind::BadLink(fault),
            }
        } else {
            SheetProblemKind::NotALink
        };
        read.problems.push(SheetProblem {
            line: index + 1,
            kind,
        });
    }
    read
}

fn escape(note: &str) -> String {
    note.replace("\r\n", "\n")
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') | None => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;

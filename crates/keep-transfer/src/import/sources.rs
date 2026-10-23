//! Turning what each source reads into import candidates.

use keep_keyuri::{ParsedKeyUri, SheetRead};

use super::{Candidate, Origin};
use crate::migration::MigrationBatch;

/// Candidates from a link sheet, with the line of each.
#[must_use]
pub fn from_sheet(read: &SheetRead) -> Vec<Candidate> {
    read.items
        .iter()
        .map(|item| Candidate {
            title: item.title.clone().unwrap_or_default(),
            issuer: item.uri.issuer.clone(),
            account: item.uri.account.clone(),
            secret: item.uri.secret.clone(),
            spec: item.uri.spec,
            counter: item.uri.counter,
            note: item.note.clone().unwrap_or_default(),
            tags: Vec::new(),
            origin: Origin::Line(item.line),
        })
        .collect()
}

/// Candidates from the codes of one Google Authenticator export, in code order.
#[must_use]
pub fn from_migration(codes: &[MigrationBatch]) -> Vec<Candidate> {
    codes
        .iter()
        .flat_map(|code| {
            code.items
                .iter()
                .enumerate()
                .map(move |(index, item)| Candidate {
                    title: String::new(),
                    issuer: item.issuer.clone(),
                    account: item.account.clone(),
                    secret: item.secret.clone(),
                    spec: item.spec,
                    counter: item.counter,
                    note: String::new(),
                    tags: Vec::new(),
                    origin: Origin::Code {
                        code: code.batch_index,
                        index,
                    },
                })
        })
        .collect()
}

/// The candidate from one pasted or scanned `otpauth://` link.
#[must_use]
pub fn from_link(parsed: &ParsedKeyUri) -> Candidate {
    Candidate {
        title: String::new(),
        issuer: parsed.uri.issuer.clone(),
        account: parsed.uri.account.clone(),
        secret: parsed.uri.secret.clone(),
        spec: parsed.uri.spec,
        counter: parsed.uri.counter,
        note: String::new(),
        tags: Vec::new(),
        origin: Origin::Item(0),
    }
}

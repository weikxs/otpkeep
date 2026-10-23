//! `otpkeep import`: adding the accounts of a file through the import preview (PRD FR-6.6).

use std::path::Path;

use keep_transfer::import::{Choice, ImportPreview, commit_import, preview};
use serde_json::json;

use crate::args::{Options, Transfer};
use crate::ask::{backup_password, confirm};
use crate::commands::{Printed, service_error};
use crate::open::open_service;
use crate::sources::read_source;
use crate::{CliError, Context, EXIT_INVALID_INPUT, EXIT_VAULT_ERROR};

/// Larger files are no import source OTPKeep writes or reads.
const MAX_IMPORT_BYTES: u64 = 32 * 1024 * 1024;

/// Reads `path`, shows what is new, and adds it after confirmation; nothing existing changes.
pub(crate) fn import(
    context: &mut Context<'_>,
    path: &Path,
    transfer: &Transfer,
    options: Options,
) -> Result<Printed, CliError> {
    let bytes = read_file(path)?;
    let source = read_source(&bytes, || {
        backup_password(context, transfer.backup_password_file.as_deref(), false)
    })?;
    let mut service = open_service(context, options)?;
    let preview = preview(source.candidates, service.doc());
    let mut notes = source.skipped;
    notes.extend(shortened(&preview));
    let summary = format!(
        "{} new, {} already in the vault",
        preview.fresh.len(),
        preview.duplicates.len()
    );
    let titles: Vec<&str> = preview
        .fresh
        .iter()
        .map(|f| f.candidate.title.as_str())
        .collect();
    if !titles.is_empty() {
        confirm(
            context,
            transfer.yes,
            &format!("{summary}. Add {}?", titles.join(", ")),
        )?;
        let (doc, _) = commit_import(
            service.doc(),
            &preview,
            &Choice::all(&preview),
            (context.now_ms)(),
        )
        .map_err(|error| CliError::new(EXIT_VAULT_ERROR, "VAULT_ERROR", &error.to_string()))?;
        service.commit_derived(doc).map_err(service_error)?;
    }
    let text = titles.iter().fold(format!("{summary}\n"), |text, title| {
        text + "+ " + title + "\n"
    });
    Ok(Printed {
        text,
        json: json!({
            "added": titles,
            "already_present": preview.duplicates.len(),
            "skipped": notes,
        }),
        notes,
    })
}

fn read_file(path: &Path) -> Result<Vec<u8>, CliError> {
    let unreadable = || {
        CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            &format!("cannot read '{}'", path.display()),
        )
    };
    let size = std::fs::metadata(path).map_err(|_| unreadable())?.len();
    if size > MAX_IMPORT_BYTES {
        return Err(CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "the file is too large to be an import source",
        ));
    }
    std::fs::read(path).map_err(|_| unreadable())
}

fn shortened(preview: &ImportPreview) -> Vec<String> {
    preview
        .fresh
        .iter()
        .filter(|f| !f.lost.is_empty())
        .map(|f| {
            format!(
                "'{}' was shortened to fit OTPKeep's limits",
                f.candidate.title
            )
        })
        .collect()
}

//! `otpkeep export`: writing every entry to a new file (PRD FR-6.1–6.3, design/05-transfer.md).

use std::path::Path;

use keep_keyuri::{KeyUri, SheetEntry, write_key_uri, write_sheet};
use keep_transfer::backup::write_backup;
use keep_transfer::goldenpassport::{GpExportItem, GpIncompatible, write_secrets};
use keep_transfer::migration::{BATCH_BUDGET_BYTES, ExportItem, Incompatible, encode_migration};
use keep_transfer::qr::{QrEcc, qr_svg};
use keep_vault::Entry;
use serde_json::json;
use zeroize::Zeroizing;

use crate::args::{ExportFormat, Options, Transfer};
use crate::ask::{backup_password, confirm};
use crate::commands::Printed;
use crate::files::{create_new_dir, ensure_absent, write_new_file};
use crate::open::open_service;
use crate::{CliError, Context, EXIT_INVALID_INPUT, EXIT_VAULT_ERROR};

/// Writes the entries (not the trash) in `format` to `out`, which must not exist.
pub(crate) fn export(
    context: &mut Context<'_>,
    format: ExportFormat,
    out: &Path,
    transfer: &Transfer,
    options: Options,
) -> Result<Printed, CliError> {
    ensure_absent(out)?;
    let service = open_service(context, options)?;
    let entries = service.ordered();
    if format.is_plaintext() {
        confirm(
            context,
            transfer.yes,
            &format!(
                "'{}' will hold {} secrets unencrypted; anyone who can read it can make your codes. Write it?",
                out.display(),
                entries.len()
            ),
        )?;
    }
    let notes = match format {
        ExportFormat::Backup => {
            let password =
                backup_password(context, transfer.backup_password_file.as_deref(), true)?;
            let params = (context.new_kdf)().ok_or_else(random_unavailable)?;
            let bytes = write_backup(&entries, &password, params).map_err(|error| {
                CliError::new(EXIT_INVALID_INPUT, "INVALID_INPUT", &error.to_string())
            })?;
            write_new_file(out, &bytes)?;
            Vec::new()
        }
        ExportFormat::Uris => {
            write_new_file(out, links_sheet(&entries).as_bytes())?;
            Vec::new()
        }
        ExportFormat::GoldenPassport => goldenpassport(&entries, out)?,
        ExportFormat::Google => google(&entries, out)?,
    };
    let written = entries.len() - notes.iter().filter(|n| n.left_out).count();
    Ok(Printed {
        text: format!("Exported {written} entries to '{}'.\n", out.display()),
        json: json!({ "exported": written, "path": out.display().to_string() }),
        notes: notes.into_iter().map(|n| n.text).collect(),
    })
}

/// A warning about one entry.
struct Note {
    text: String,
    /// The entry is not in the export at all.
    left_out: bool,
}

fn key_uri(entry: &Entry) -> KeyUri {
    let key = &entry.key.value;
    KeyUri {
        secret: key.secret.clone(),
        spec: key.spec,
        counter: key.counter,
        issuer: entry.issuer.value.clone(),
        account: entry.account.value.clone(),
    }
}

fn links_sheet(entries: &[&Entry]) -> Zeroizing<String> {
    let uris: Vec<KeyUri> = entries.iter().map(|e| key_uri(e)).collect();
    let sheet: Vec<SheetEntry<'_>> = entries
        .iter()
        .zip(&uris)
        .map(|(e, uri)| SheetEntry {
            title: &e.title.value,
            note: &e.note.value,
            uri,
        })
        .collect();
    write_sheet(&sheet)
}

fn goldenpassport(entries: &[&Entry], out: &Path) -> Result<Vec<Note>, CliError> {
    let uris: Vec<KeyUri> = entries.iter().map(|e| key_uri(e)).collect();
    let items: Vec<GpExportItem<'_>> = entries
        .iter()
        .zip(&uris)
        .map(|(e, uri)| GpExportItem {
            name: &e.title.value,
            uri,
        })
        .collect();
    let export = write_secrets(&items);
    write_new_file(out, &export.bytes)?;
    Ok(export
        .skipped
        .iter()
        .map(|(index, reason)| Note {
            text: format!(
                "left out '{}': GoldenPassport would show wrong codes ({})",
                entries[*index].title.value,
                gp_reason(*reason)
            ),
            left_out: true,
        })
        .collect())
}

fn gp_reason(reason: GpIncompatible) -> &'static str {
    match reason {
        GpIncompatible::CounterBased => "it is counter-based (HOTP)",
        GpIncompatible::Algorithm => "it does not use SHA-1",
        GpIncompatible::Digits => "it does not have 6 digits",
        GpIncompatible::Period => "it does not change every 30 seconds",
        GpIncompatible::Steam => "it is a Steam Guard code",
    }
}

/// A folder with the transfer codes, and a single-account code for each account they cannot hold.
fn google(entries: &[&Entry], out: &Path) -> Result<Vec<Note>, CliError> {
    let items: Vec<ExportItem<'_>> = entries
        .iter()
        .map(|e| ExportItem {
            secret: &e.key.value.secret,
            spec: e.key.value.spec,
            counter: e.key.value.counter,
            issuer: &e.issuer.value,
            account: &e.account.value,
        })
        .collect();
    let export = encode_migration(&items, BATCH_BUDGET_BYTES).map_err(|_| random_unavailable())?;
    create_new_dir(out)?;
    let count = export.links.len();
    for (index, link) in export.links.iter().enumerate() {
        let name = format!("google-{}-of-{count}.svg", index + 1);
        write_svg(&out.join(name), link, QrEcc::Low)?;
    }
    let mut notes = Vec::new();
    for (number, (index, reason)) in export.separate.iter().enumerate() {
        let title = &entries[*index].title.value;
        let name = format!("single-{}.svg", number + 1);
        write_svg(
            &out.join(&name),
            &write_key_uri(&key_uri(entries[*index])),
            QrEcc::Medium,
        )?;
        notes.push(Note {
            text: format!(
                "'{title}' cannot go into a transfer code ({}); scan {name} on its own",
                google_reason(*reason)
            ),
            left_out: false,
        });
    }
    notes.extend(export.algorithm_unverified.iter().map(|index| Note {
        text: format!(
            "'{}' uses SHA-256 or SHA-512; check its codes after scanning",
            entries[*index].title.value
        ),
        left_out: false,
    }));
    Ok(notes)
}

fn google_reason(reason: Incompatible) -> String {
    match reason {
        Incompatible::Period(seconds) => format!("its codes change every {seconds} seconds"),
        Incompatible::Digits(digits) => format!("it has {digits} digits"),
        Incompatible::Md5 => "it uses MD5".to_owned(),
        Incompatible::Steam => {
            "it is a Steam Guard code, which Google Authenticator cannot show".to_owned()
        }
    }
}

fn write_svg(path: &Path, text: &str, ecc: QrEcc) -> Result<(), CliError> {
    let svg = qr_svg(text, ecc)
        .map_err(|error| CliError::new(EXIT_VAULT_ERROR, "VAULT_ERROR", &error.to_string()))?;
    write_new_file(path, svg.as_bytes())
}

fn random_unavailable() -> CliError {
    CliError::new(
        EXIT_VAULT_ERROR,
        "VAULT_ERROR",
        "the system random number generator is unavailable",
    )
}

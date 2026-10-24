//! Turning vault entries into the export formats (PRD FR-6.1–6.3, design/05-transfer.md). Shared by the
//! command line and the app, so both write the same files.

use keep_keyuri::{KeyUri, SheetEntry, write_key_uri, write_sheet};
use keep_vault::Entry;
use zeroize::Zeroizing;

use crate::goldenpassport::{GpExport, GpExportItem, write_secrets};
use crate::migration::{
    BATCH_BUDGET_BYTES, ExportError, ExportItem, MigrationExport, encode_migration,
};

/// The link fields of an entry.
#[must_use]
pub fn key_uri_of(entry: &Entry) -> KeyUri {
    let key = &entry.key.value;
    KeyUri {
        secret: key.secret.clone(),
        spec: key.spec,
        counter: key.counter,
        issuer: entry.issuer.value.clone(),
        account: entry.account.value.clone(),
    }
}

/// One entry's link, for a single-account QR code. It holds the secret and is wiped when dropped.
#[must_use]
pub fn single_link(entry: &Entry) -> Zeroizing<String> {
    write_key_uri(&key_uri_of(entry))
}

/// A link sheet with each entry's name and note (design/05-transfer.md §3).
#[must_use]
pub fn links_sheet(entries: &[&Entry]) -> Zeroizing<String> {
    let uris: Vec<KeyUri> = entries.iter().map(|e| key_uri_of(e)).collect();
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

/// A GoldenPassport `.secrets` file; entries it would compute wrongly are left out and listed.
#[must_use]
pub fn goldenpassport_file(entries: &[&Entry]) -> GpExport {
    let uris: Vec<KeyUri> = entries.iter().map(|e| key_uri_of(e)).collect();
    let items: Vec<GpExportItem<'_>> = entries
        .iter()
        .zip(&uris)
        .map(|(e, uri)| GpExportItem {
            name: &e.title.value,
            uri,
        })
        .collect();
    write_secrets(&items)
}

/// Google Authenticator transfer codes; entries they cannot hold are listed to be shown one by one.
///
/// # Errors
///
/// Returns [`ExportError`] when the system random source fails.
pub fn google_codes(entries: &[&Entry]) -> Result<MigrationExport, ExportError> {
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
    encode_migration(&items, BATCH_BUDGET_BYTES)
}

#[cfg(test)]
mod tests {
    use keep_passcode::{CodeSpec, SecretKey};
    use keep_vault::VaultDoc;
    use keep_vault::edit::{NewEntry, add_entry};

    use super::{goldenpassport_file, google_codes, links_sheet, single_link};
    use crate::import::{from_sheet, preview};

    fn doc() -> VaultDoc {
        let mut doc = VaultDoc::blank();
        for (title, byte, spec) in [
            ("Demo", 1u8, CodeSpec::default()),
            ("Steam", 2, CodeSpec::steam(30).unwrap()),
        ] {
            let new = NewEntry {
                title: title.to_owned(),
                issuer: title.to_owned(),
                account: "alex".to_owned(),
                secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
                spec,
                counter: 0,
                note: "备注".to_owned(),
                tags: Vec::new(),
                keep_duplicate: false,
            };
            doc = add_entry(&doc, new, 1).unwrap().0;
        }
        doc
    }

    #[test]
    fn a_link_sheet_reads_back_with_names_and_notes() {
        let doc = doc();
        let entries: Vec<_> = doc.entries.iter().collect();

        let sheet = links_sheet(&entries);

        let read = keep_keyuri::read_sheet(&sheet);
        let back = preview(from_sheet(&read), &VaultDoc::blank());
        assert_eq!(back.fresh.len(), 2);
        assert_eq!(back.fresh[0].candidate.note, "备注");
        assert!(back.fresh[1].candidate.spec.is_steam());
    }

    #[test]
    fn goldenpassport_and_google_leave_steam_out() {
        let doc = doc();
        let entries: Vec<_> = doc.entries.iter().collect();

        assert_eq!(goldenpassport_file(&entries).skipped.len(), 1);
        let google = google_codes(&entries).unwrap();
        assert_eq!(google.links.len(), 1);
        assert_eq!(google.separate.len(), 1);
    }

    #[test]
    fn a_single_link_is_the_entry() {
        let doc = doc();

        assert!(single_link(&doc.entries[1]).starts_with("otpauth://steam/Steam:alex?secret="));
    }
}

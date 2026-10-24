use keep_keyuri::KeyUriFault;
use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::edit::{NewEntry, add_entry};
use keep_vault::sealed::KdfParams;
use keep_vault::{DeviceTag, Extra, Hlc, VaultDoc};

use super::{Place, SkipReason, SourceError, read_source, read_text};
use crate::backup::{BackupError, write_backup};
use crate::migration::{BATCH_BUDGET_BYTES, ExportItem, MigrationFault, encode_migration};
use crate::qr::tests::{picture, png};

const DEMO: &str = "JBSWY3DPEHPK3PXP";

fn doc_with(title: &str) -> VaultDoc {
    let doc = VaultDoc {
        vault_id: [0; 16],
        entries: vec![],
        tombstones: vec![],
        group: None,
        device: None,
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([1; 8]),
        extra: Extra::new(),
    };
    let new = NewEntry {
        title: title.to_owned(),
        issuer: "Demo".to_owned(),
        account: "alex".to_owned(),
        secret: SecretKey::from_bytes(vec![7; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: Vec::new(),
        keep_duplicate: false,
    };
    add_entry(&doc, new, 1).unwrap().0
}

fn quick() -> KdfParams {
    KdfParams {
        salt: [1; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

#[test]
fn reads_a_pasted_link() {
    let source = read_text(&format!(
        "otpauth://totp/Demo:alex?secret={DEMO}&issuer=Demo"
    ));

    assert_eq!(source.candidates.len(), 1);
    assert_eq!(source.candidates[0].issuer, "Demo");
    assert!(source.skipped.is_empty());
}

#[test]
fn reads_several_lines_and_names_each_line_it_skips() {
    let source = read_text(&format!(
        "# Work\notpauth://totp/A?secret={DEMO}\nhello\nphonefactor://activate_account?code=1\n{DEMO}\n"
    ));

    assert_eq!(source.candidates.len(), 1);
    assert_eq!(source.candidates[0].title, "Work");
    let skipped: Vec<(Place, SkipReason)> = source
        .skipped
        .into_iter()
        .map(|s| (s.place, s.reason))
        .collect();
    assert_eq!(
        skipped,
        [
            (Place::Line(3), SkipReason::NotALink),
            (Place::Line(4), SkipReason::Link(KeyUriFault::MicrosoftPush)),
            (Place::Line(5), SkipReason::Link(KeyUriFault::BareSecret)),
        ]
    );
}

#[test]
fn reads_a_google_transfer_code_even_with_words_around_it() {
    let secret = SecretKey::from_bytes(vec![9; 20]).unwrap();
    let items = [ExportItem {
        secret: &secret,
        spec: CodeSpec::default(),
        counter: 0,
        issuer: "Demo",
        account: "alex",
    }];
    let export = encode_migration(&items, BATCH_BUDGET_BYTES).unwrap();

    let source = read_text(&format!("导出码：{} 请保存", export.links[0].as_str()));

    assert_eq!(source.candidates.len(), 1);
    assert_eq!(source.candidates[0].account, "alex");
}

#[test]
fn names_a_broken_transfer_code() {
    let source = read_text("otpauth-migration://offline?data=%%%");

    assert_eq!(source.skipped.len(), 1);
    assert_eq!(source.skipped[0].place, Place::Line(1));
    assert!(matches!(
        source.skipped[0].reason,
        SkipReason::Transfer(MigrationFault::BadBase64)
    ));
}

#[test]
fn a_backup_asks_for_its_password_first() {
    let doc = doc_with("Backed up");
    let bytes = write_backup(&doc.entries.iter().collect::<Vec<_>>(), "pw", quick()).unwrap();

    assert_eq!(read_source(&bytes, None), Err(SourceError::NeedsPassword));
    assert_eq!(
        read_source(&bytes, Some("wrong")),
        Err(SourceError::Backup(BackupError::WrongPassword))
    );
    let source = read_source(&bytes, Some("pw")).unwrap();
    assert_eq!(source.candidates[0].title, "Backed up");
}

#[test]
fn reads_every_qr_code_in_a_picture() {
    let first = format!("otpauth://totp/Pic:alex?secret={DEMO}");
    let second = "otpauth://totp/Pic:sam?secret=GEZDGNBVGY3TQOJQ";
    let bytes = png(&picture(&[&first, second], 4));

    let source = read_source(&bytes, None).unwrap();

    let accounts: Vec<&str> = source
        .candidates
        .iter()
        .map(|c| c.account.as_str())
        .collect();
    assert_eq!(accounts.len(), 2);
    assert!(accounts.contains(&"alex") && accounts.contains(&"sam"));
}

#[test]
fn reads_goldenpassport_json() {
    let json = format!(
        r#"[{{"id":"00000000-0000-4000-8000-000000000001","name":"GP demo","url":"otpauth://totp/GP:alex?secret={DEMO}"}}]"#
    );

    let source = read_source(json.as_bytes(), None).unwrap();

    assert_eq!(source.candidates.len(), 1);
    assert_eq!(source.candidates[0].title, "GP demo");
}

#[test]
fn refuses_what_it_does_not_know() {
    assert_eq!(
        read_source(&[0xff, 0xfe, 0x00, 0x01], None),
        Err(SourceError::Unknown)
    );
}

#[test]
fn an_empty_text_gives_nothing() {
    let source = read_text("  \n\n");

    assert!(source.candidates.is_empty());
    assert!(source.skipped.is_empty());
}

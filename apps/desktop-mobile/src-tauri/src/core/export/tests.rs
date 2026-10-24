use std::sync::Arc;
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_transfer::source::read_source;
use keep_vault::sealed::KdfParams;
use keep_vault::store::VaultPaths;

use super::{ExportFormat, date_of};
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_793_000_000_000;

fn quick() -> KdfParams {
    KdfParams {
        salt: [8; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-export-")
        .tempdir()
        .unwrap()
}

fn with_two(dir: &std::path::Path) -> (Core, Vec<String>) {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );
    let view = app
        .stage_text(
            "# 工作邮箱\notpauth://totp/Mail:alex?secret=JBSWY3DPEHPK3PXP\n\
             otpauth://steam/Steam:player?secret=GEZDGNBVGY3TQOJQ",
        )
        .unwrap();
    let all = Choice {
        fresh: vec![true; 2],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &all).unwrap();
    let ids = app.list("").unwrap().into_iter().map(|e| e.id).collect();
    (app, ids)
}

#[test]
fn names_files_by_date() {
    assert_eq!(date_of(0), "1970-01-01");
    assert_eq!(date_of(951_782_400_000), "2000-02-29");
    assert_eq!(date_of(NOW_MS), "2026-10-26");
}

#[test]
fn a_backup_reads_back_with_its_password() {
    let dir = temp_dir();
    let (app, _) = with_two(dir.path());

    let file = app
        .export_file_with(ExportFormat::Backup, None, Some("备份口令"), quick())
        .unwrap();

    assert_eq!(file.file_name, "otpkeep-backup-2026-10-26.otpkeep");
    assert_eq!(file.count, 2);
    let back = read_source(&file.bytes, Some("备份口令")).unwrap();
    assert_eq!(back.candidates.len(), 2);
    assert_eq!(
        app.export_file_with(ExportFormat::Backup, None, Some(""), quick())
            .err(),
        Some(CoreError::Invalid("BACKUP_PASSWORD_EMPTY"))
    );
}

#[test]
fn a_link_list_keeps_names_and_steam() {
    let dir = temp_dir();
    let (app, _) = with_two(dir.path());

    let file = app
        .export_file_with(ExportFormat::Links, None, None, quick())
        .unwrap();

    let text = String::from_utf8(file.bytes.to_vec()).unwrap();
    assert!(text.contains("# 工作邮箱"));
    assert!(text.contains("otpauth://steam/"));
    assert!(ExportFormat::Links.is_plaintext() && !ExportFormat::Backup.is_plaintext());
}

#[test]
fn exports_only_the_chosen_entries() {
    let dir = temp_dir();
    let (app, ids) = with_two(dir.path());

    let file = app
        .export_file_with(ExportFormat::Links, Some(&ids[..1]), None, quick())
        .unwrap();

    assert_eq!(file.count, 1);
    assert_eq!(
        app.export_file_with(
            ExportFormat::Links,
            Some(&["gone".to_owned()]),
            None,
            quick()
        )
        .err(),
        Some(CoreError::NotFound)
    );
    assert_eq!(
        app.export_file_with(ExportFormat::Links, Some(&[]), None, quick())
            .err(),
        Some(CoreError::Invalid("NOTHING_TO_EXPORT"))
    );
}

#[test]
fn goldenpassport_leaves_steam_out_and_says_so() {
    let dir = temp_dir();
    let (app, _) = with_two(dir.path());

    let file = app
        .export_file_with(ExportFormat::GoldenPassport, None, None, quick())
        .unwrap();

    assert_eq!(file.count, 1);
    assert_eq!(file.left_out[0].title, "Steam");
    assert_eq!(file.left_out[0].reason, "STEAM");
}

#[test]
fn google_codes_are_pictures_with_steam_shown_on_its_own() {
    let dir = temp_dir();
    let (app, _) = with_two(dir.path());

    let google = app.export_google(None).unwrap();

    assert_eq!(google.codes.len(), 1);
    // Shown as an image (a data: address), which needs the SVG namespace.
    assert!(
        google.codes[0]
            .svg
            .contains(r#"xmlns="http://www.w3.org/2000/svg""#)
    );
    assert_eq!(google.separate[0].reason, "STEAM");
}

#[test]
fn one_entry_has_its_own_qr_code() {
    let dir = temp_dir();
    let (app, ids) = with_two(dir.path());

    assert!(app.entry_qr(&ids[0]).unwrap().svg.contains("svg"));
    assert_eq!(app.entry_qr("gone").err(), Some(CoreError::NotFound));
}

#[test]
fn nothing_to_export_from_an_empty_start() {
    let dir = temp_dir();
    let app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );

    assert_eq!(
        app.export_google(None).err(),
        Some(CoreError::NeedsPassword)
    );
}

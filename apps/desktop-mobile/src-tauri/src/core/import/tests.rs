use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeSpec, SecretKey};
use keep_platform::MemoryKeystore;
use keep_transfer::backup::write_backup;
use keep_transfer::import::Choice;
use keep_vault::open::create_vault;
use keep_vault::sealed::KdfParams;
use keep_vault::store::VaultPaths;

use super::{ImportProblem, ManualForm};
use crate::core::import_view::RowGroup;
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;
const DEMO: &str = "JBSWY3DPEHPK3PXP";

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-import-")
        .tempdir()
        .unwrap()
}

/// A core on a fresh data directory: nothing exists yet.
fn fresh(dir: &Path) -> Core {
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    )
}

fn choose(fresh: usize, duplicates: usize) -> Choice {
    Choice {
        fresh: vec![true; fresh],
        duplicates: vec![false; duplicates],
    }
}

fn form(secret: &str, kind: &str) -> ManualForm {
    ManualForm {
        title: String::new(),
        issuer: "Example".to_owned(),
        account: "alex".to_owned(),
        secret: secret.to_owned(),
        kind: kind.to_owned(),
        algorithm: "SHA1".to_owned(),
        digits: 6,
        period_s: 30,
        counter: 0,
    }
}

#[test]
fn a_pasted_link_is_previewed_with_its_current_code_and_the_first_commit_creates_the_vault() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let view = app
        .stage_text(&format!(
            "请扫描：otpauth://totp/Demo:alex?secret={DEMO}&issuer=Demo"
        ))
        .unwrap();

    assert_eq!(view.rows.len(), 1);
    let row = &view.rows[0];
    assert_eq!(
        (row.group, row.kind, row.issuer.as_str()),
        (RowGroup::Fresh, "totp", "Demo")
    );
    assert_eq!(row.sample_code.as_ref().map(String::len), Some(6));
    assert!(
        !VaultPaths::new(dir.path()).vault().exists(),
        "nothing is created by a preview"
    );

    assert_eq!(app.commit_staged(&view.token, &choose(1, 0)), Ok(1));
    assert!(VaultPaths::new(dir.path()).vault().exists());
    assert_eq!(app.list("").unwrap()[0].issuer, "Demo");
}

#[test]
fn the_view_never_carries_a_secret() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let view = app
        .stage_text(&format!("otpauth://totp/Demo:alex?secret={DEMO}"))
        .unwrap();

    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains(DEMO));
    assert!(!json.to_lowercase().contains("secret"));
}

#[test]
fn an_account_already_in_the_vault_is_a_duplicate_named_after_the_entry() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());
    let first = app
        .stage_text(&format!("otpauth://totp/Demo:alex?secret={DEMO}"))
        .unwrap();
    app.commit_staged(&first.token, &choose(1, 0)).unwrap();

    let again = app
        .stage_text(&format!("otpauth://totp/Other:sam?secret={DEMO}"))
        .unwrap();

    assert_eq!(again.rows[0].group, RowGroup::Duplicate);
    assert_eq!(again.rows[0].existing_title.as_deref(), Some("Demo"));
    assert_eq!(app.commit_staged(&again.token, &choose(0, 1)), Ok(0));
}

#[test]
fn explains_lines_it_could_not_read() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let view = app
        .stage_text("phonefactor://activate_account?code=1\nJBSW0Y3DP")
        .unwrap();

    let reasons: Vec<&str> = view.skipped.iter().map(|s| s.reason).collect();
    assert_eq!(reasons, ["MICROSOFT_PUSH", "NOT_A_LINK"]);
    assert!(view.rows.is_empty());
}

#[test]
fn a_backup_asks_for_its_password_then_reads() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    let source_dir = temp_dir();
    let open = create_vault(&VaultPaths::new(source_dir.path()), &keystore, WAIT).unwrap();
    let mut service = keep_vault::service::VaultService::new(open, || NOW_MS);
    service
        .add(keep_vault::edit::NewEntry {
            title: "Backed up".to_owned(),
            issuer: "Demo".to_owned(),
            account: "alex".to_owned(),
            secret: SecretKey::from_bytes(vec![5; 20]).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            note: String::new(),
            tags: Vec::new(),
            keep_duplicate: false,
        })
        .unwrap();
    let quick = KdfParams {
        salt: [4; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let bytes = write_backup(
        &service.doc().entries.iter().collect::<Vec<_>>(),
        "pw",
        quick,
    )
    .unwrap();
    let mut app = fresh(dir.path());

    assert_eq!(
        app.stage_bytes(&bytes, None),
        Err(CoreError::Import(ImportProblem::NeedsBackupPassword))
    );
    assert_eq!(
        app.stage_bytes(&bytes, Some("wrong")),
        Err(CoreError::Import(ImportProblem::WrongBackupPassword))
    );
    assert_eq!(
        app.stage_bytes(&bytes, Some("pw")).unwrap().rows[0].title,
        "Backed up"
    );
    assert_eq!(
        app.stage_bytes(b"\xff\xfe\x00", None),
        Err(CoreError::Import(ImportProblem::UnknownFormat))
    );
}

#[test]
fn the_manual_form_shows_a_code_to_compare_before_adding() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let view = app
        .stage_manual(&form("jbsw y3dp ehpk 3pxp", "totp"))
        .unwrap();

    assert_eq!(view.rows[0].sample_code.as_ref().map(String::len), Some(6));
    assert_eq!(view.rows[0].title, "Example");
    assert_eq!(app.commit_staged(&view.token, &choose(1, 0)), Ok(1));
}

#[test]
fn the_manual_form_points_at_a_bad_character_and_needs_a_name() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    assert_eq!(
        app.stage_manual(&form("JBSW0Y3DP", "totp")),
        Err(CoreError::SecretInvalid { at: 4, found: '0' })
    );
    assert_eq!(
        app.stage_manual(&form("", "totp")),
        Err(CoreError::Invalid("SECRET_MISSING"))
    );
    let mut nameless = form(DEMO, "totp");
    nameless.issuer.clear();
    nameless.account.clear();
    assert_eq!(
        app.stage_manual(&nameless),
        Err(CoreError::Invalid("NAME_MISSING"))
    );
    assert_eq!(
        app.stage_manual(&{
            let mut nine = form(DEMO, "totp");
            nine.digits = 9;
            nine
        }),
        Err(CoreError::Invalid("DIGITS"))
    );
    assert_eq!(
        app.stage_manual(&form(DEMO, "yandex")),
        Err(CoreError::Invalid("KIND"))
    );
}

#[test]
fn the_manual_form_adds_steam_guard() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let view = app.stage_manual(&form(DEMO, "steam")).unwrap();

    assert_eq!(view.rows[0].kind, "steam");
    let code = view.rows[0].sample_code.clone().unwrap();
    assert_eq!(code.len(), 5);
    assert!(
        code.chars()
            .all(|c| "23456789BCDFGHJKMNPQRTVWXY".contains(c))
    );
}

#[test]
fn counter_codes_show_no_sample_so_the_counter_does_not_move() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());

    let mut counted = form(DEMO, "hotp");
    counted.counter = 7;
    let view = app.stage_manual(&counted).unwrap();

    assert_eq!(view.rows[0].sample_code, None);
    assert_eq!(view.rows[0].counter, Some(7));
}

#[test]
fn a_vault_change_since_the_preview_previews_again_under_the_same_token() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());
    let waiting = app
        .stage_text(&format!("otpauth://totp/Late:alex?secret={DEMO}"))
        .unwrap();
    let meanwhile = app
        .stage_text(&format!("otpauth://totp/First:alex?secret={DEMO}"))
        .unwrap();
    app.commit_staged(&meanwhile.token, &choose(1, 0)).unwrap();

    assert_eq!(
        app.commit_staged(&waiting.token, &choose(1, 0)),
        Err(CoreError::Import(ImportProblem::Changed))
    );
    let again = app.staged_view(&waiting.token).unwrap();
    assert_eq!(again.rows[0].group, RowGroup::Duplicate);
    assert_eq!(again.rows[0].existing_title.as_deref(), Some("First"));
}

#[test]
fn a_cancelled_or_committed_import_is_gone() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());
    let view = app
        .stage_text(&format!("otpauth://totp/Demo:alex?secret={DEMO}"))
        .unwrap();

    app.cancel_staged(&view.token);

    assert_eq!(
        app.commit_staged(&view.token, &choose(1, 0)),
        Err(CoreError::Import(ImportProblem::NotStaged))
    );
}

#[test]
fn a_choice_that_does_not_fit_is_refused_and_nothing_is_added() {
    let dir = temp_dir();
    let mut app = fresh(dir.path());
    let view = app
        .stage_text(&format!("otpauth://totp/Demo:alex?secret={DEMO}"))
        .unwrap();

    assert_eq!(
        app.commit_staged(&view.token, &choose(3, 0)),
        Err(CoreError::Invalid("CHOICE_MISMATCH"))
    );
    assert!(app.list("").unwrap().is_empty());
}

#[test]
fn nothing_is_staged_before_the_master_password() {
    let dir = temp_dir();
    let params = KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    drop(
        keep_vault::open::create_with_master_password(
            &VaultPaths::new(dir.path()),
            "pw",
            params,
            WAIT,
        )
        .unwrap(),
    );
    let mut app = fresh(dir.path());

    assert_eq!(app.stage_text("anything"), Err(CoreError::NeedsPassword));
}

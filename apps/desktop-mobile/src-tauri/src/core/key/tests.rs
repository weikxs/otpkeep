use std::sync::Arc;
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::store::VaultPaths;

use super::KeyForm;
use crate::core::{Core, CoreError};

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-key-")
        .tempdir()
        .unwrap()
}

/// A vault with one TOTP entry, "Demo".
fn with_one(dir: &std::path::Path) -> (Core, String) {
    let mut app = Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );
    let view = app
        .stage_text("otpauth://totp/Demo:alex?secret=JBSWY3DPEHPK3PXP")
        .unwrap();
    let one = Choice {
        fresh: vec![true],
        duplicates: Vec::new(),
    };
    app.commit_staged(&view.token, &one).unwrap();
    let id = app.list("").unwrap().remove(0).id;
    (app, id)
}

fn form(secret: &str, kind: &str, digits: u8) -> KeyForm {
    KeyForm {
        secret: secret.to_owned(),
        kind: kind.to_owned(),
        algorithm: "SHA1".to_owned(),
        digits,
        period_s: 30,
        counter: 0,
    }
}

fn current_code(app: &Core) -> String {
    app.state_all("").entries[0].code.clone().unwrap()
}

#[test]
fn describes_the_key_without_the_secret() {
    let dir = temp_dir();
    let (app, id) = with_one(dir.path());

    let info = app.key_info(&id).unwrap();

    assert_eq!(
        (info.kind, info.algorithm, info.digits, info.period_s),
        ("totp", "SHA1", 6, Some(30))
    );
    assert!(!info.short_secret);
    assert!(!serde_json::to_string(&info).unwrap().contains("JBSW"));
}

#[test]
fn previews_new_parameters_before_saving_them() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = current_code(&app);

    let eight = app.preview_key(&id, &form("", "totp", 8)).unwrap().unwrap();

    assert_eq!(eight.len(), 8);
    assert_eq!(current_code(&app), before, "a preview changes nothing");
    app.replace_key(&id, &form("", "totp", 8)).unwrap();
    assert_eq!(current_code(&app), eight);
    assert_eq!(app.key_info(&id).unwrap().digits, 8);
}

#[test]
fn replaces_the_secret_and_the_codes_follow() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = current_code(&app);

    app.replace_key(&id, &form("GEZD GNBV GY3T QOJQ", "totp", 6))
        .unwrap();

    assert_ne!(current_code(&app), before);
}

#[test]
fn changes_to_steam_or_counter_codes() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());

    assert_eq!(
        app.preview_key(&id, &form("", "steam", 6))
            .unwrap()
            .unwrap()
            .len(),
        5
    );
    let mut counter = form("", "hotp", 6);
    counter.counter = 9;
    assert_eq!(app.preview_key(&id, &counter).unwrap(), None);
    app.replace_key(&id, &counter).unwrap();
    assert_eq!(app.key_info(&id).unwrap().counter, Some(9));
}

#[test]
fn refuses_bad_fields_and_changes_nothing() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());
    let before = current_code(&app);

    assert_eq!(
        app.replace_key(&id, &form("", "totp", 9)),
        Err(CoreError::Invalid("DIGITS"))
    );
    assert_eq!(
        app.replace_key(&id, &form("JBSW1", "totp", 6)),
        Err(CoreError::SecretInvalid { at: 4, found: '1' })
    );
    let mut algorithm = form("", "totp", 6);
    algorithm.algorithm = "MD5".to_owned();
    assert_eq!(
        app.preview_key(&id, &algorithm),
        Err(CoreError::Invalid("ALGORITHM"))
    );
    assert_eq!(
        app.replace_key("nope", &form("", "totp", 6)),
        Err(CoreError::NotFound)
    );
    assert_eq!(current_code(&app), before);
}

#[test]
fn renames_the_service_and_account() {
    let dir = temp_dir();
    let (mut app, id) = with_one(dir.path());

    app.rename(
        &id,
        Some("Example Cloud".to_owned()),
        Some("ops@example.com".to_owned()),
    )
    .unwrap();

    let info = app.list("").unwrap().remove(0);
    assert_eq!(
        (info.issuer.as_str(), info.account.as_str()),
        ("Example Cloud", "ops@example.com")
    );
    assert_eq!(
        app.rename(&id, Some("x".repeat(300)), None),
        Err(CoreError::Invalid("ISSUER_TOO_LONG"))
    );
}

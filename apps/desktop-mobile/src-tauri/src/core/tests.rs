use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use keep_platform::MemoryKeystore;
use keep_vault::edit::{Change, NewEntry};
use keep_vault::open::{create_vault, create_with_master_password};
use keep_vault::sealed::KdfParams;
use keep_vault::service::VaultService;
use keep_vault::store::VaultPaths;

use super::{Core, CoreError, EntryKind, PANEL_LIMIT, PanelStatus, Problem};

const WAIT: Duration = Duration::from_millis(200);
/// 10 seconds into a 30-second step.
const STEADY_MS: u64 = 1_700_000_020_000;
/// 3 seconds before the step ends.
const ENDING_MS: u64 = 1_700_000_037_000;
const PASSWORD: &str = "demo password";

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-core-")
        .tempdir_in(std::env::temp_dir())
        .unwrap()
}

fn entry(title: &str, issuer: &str, byte: u8, spec: CodeSpec) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: issuer.to_owned(),
        account: "demo-user".to_owned(),
        secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
        spec,
        counter: 0,
        note: String::new(),
        tags: vec!["demo".to_owned()],
        keep_duplicate: false,
    }
}

fn hotp() -> CodeSpec {
    CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap()
}

fn add_demo(service: &mut VaultService) {
    service
        .add(entry(
            "Example Cloud",
            "Example Cloud",
            1,
            CodeSpec::default(),
        ))
        .unwrap();
    let pinned = service
        .add(entry(
            "Example Mail",
            "Example Mail",
            2,
            CodeSpec::default(),
        ))
        .unwrap();
    service.edit(pinned, Change::Pinned(true)).unwrap();
    service
        .add(entry("Bank token", "Example Bank", 3, hotp()))
        .unwrap();
}

fn keystore_core(dir: &Path, now: u64) -> Core {
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir), &keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, move || now);
    add_demo(&mut service);
    drop(service);
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(keystore),
        Arc::new(move || now),
        WAIT,
    )
}

fn password_core(dir: &Path) -> Core {
    let params = KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let open = create_with_master_password(&VaultPaths::new(dir), PASSWORD, params, WAIT).unwrap();
    let mut service = VaultService::new(open, || STEADY_MS);
    add_demo(&mut service);
    drop(service);
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| STEADY_MS),
        WAIT,
    )
}

#[test]
fn lists_pinned_first_with_codes_and_shortcuts() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), STEADY_MS);

    let state = app.state("");

    assert_eq!(state.status, PanelStatus::Ready);
    let titles: Vec<&str> = state.entries.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, ["Example Mail", "Example Cloud", "Bank token"]);
    let first = &state.entries[0];
    assert!(first.pinned);
    assert_eq!(first.code.as_ref().map(String::len), Some(6));
    assert_eq!(first.period_s, Some(30));
    assert_eq!(first.valid_until_ms, Some(1_700_000_040_000));
    assert_eq!(first.next_code, None);
    assert_eq!(state.entries[2].kind, EntryKind::Hotp);
    assert_eq!(state.entries[2].code, None);
    assert!(!state.uses_master_password);
    assert!(!state.in_group);
}

#[test]
fn offers_the_next_code_near_the_end() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), ENDING_MS);

    let state = app.state("");

    assert!(state.entries[0].next_code.is_some());
}

#[test]
fn never_sends_secrets() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), STEADY_MS);

    let json = serde_json::to_string(&app.state("")).unwrap();

    assert!(!json.to_lowercase().contains("secret"));
    assert!(!json.contains("AQEBAQEB"), "no base32 of the demo secret");
    assert!(json.contains("\"validUntilMs\""));
}

#[test]
fn searches_by_name() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), STEADY_MS);

    let state = app.state("bank");

    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.entries[0].title, "Bank token");
}

#[test]
fn shows_at_most_the_panel_limit_without_a_search() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir.path()), &keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || STEADY_MS);
    for byte in 0..25u8 {
        service
            .add(entry(
                &format!("Demo {byte}"),
                "Demo",
                byte + 10,
                CodeSpec::default(),
            ))
            .unwrap();
    }
    drop(service);
    let app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(keystore),
        Arc::new(|| STEADY_MS),
        WAIT,
    );

    assert_eq!(app.state("").entries.len(), PANEL_LIMIT);
    assert_eq!(app.state("demo").entries.len(), 25);
    assert_eq!(app.list("").unwrap().len(), 25);
}

#[test]
fn copies_the_current_code() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), STEADY_MS);
    let first = app.state("").entries.remove(0);

    let code = app.code(&first.id).unwrap();

    assert_eq!(Some(code.as_str()), first.code.as_deref());
    assert_eq!(app.code("nope"), Err(CoreError::NotFound));
}

#[test]
fn generates_a_counter_code_only_when_asked() {
    let dir = temp_dir();
    let mut app = keystore_core(dir.path(), STEADY_MS);
    let bank = app.state("bank").entries.remove(0);
    assert_eq!(app.code(&bank.id), Err(CoreError::NoCodeYet));

    let generated = app.next_hotp(&bank.id).unwrap();

    let code = generated.code.unwrap();
    assert_eq!(app.code(&bank.id).unwrap().as_str(), code);
    let again = app.next_hotp(&bank.id).unwrap().code.unwrap();
    assert_ne!(again, code);
}

#[test]
fn starts_empty_without_creating_anything() {
    let dir = temp_dir();
    let app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| STEADY_MS),
        WAIT,
    );

    let state = app.state("");

    assert_eq!(state.status, PanelStatus::Ready);
    assert!(state.entries.is_empty());
    assert!(
        !VaultPaths::new(dir.path()).vault().exists(),
        "no vault is created"
    );
}

#[test]
fn unlocks_with_the_master_password() {
    let dir = temp_dir();
    let mut app = password_core(dir.path());
    assert_eq!(app.state("").status, PanelStatus::NeedsPassword);
    assert!(app.state("").uses_master_password);
    assert_eq!(app.code("x"), Err(CoreError::NeedsPassword));

    assert_eq!(app.unlock("wrong").status, PanelStatus::NeedsPassword);
    let state = app.unlock(PASSWORD);

    assert_eq!(state.status, PanelStatus::Ready);
    assert_eq!(state.entries.len(), 3);
}

#[test]
fn stays_open_after_the_password_and_ignores_more_unlocks() {
    let dir = temp_dir();
    let mut app = password_core(dir.path());
    app.unlock(PASSWORD);

    let state = app.unlock("wrong");

    assert_eq!(state.status, PanelStatus::Ready);
    assert_eq!(state.entries.len(), 3);
}

#[test]
fn explains_a_missing_key() {
    let dir = temp_dir();
    keystore_core(dir.path(), STEADY_MS);

    let app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| STEADY_MS),
        WAIT,
    );

    let state = app.state("");
    assert_eq!(state.status, PanelStatus::Unavailable);
    assert_eq!(state.problem, Some(Problem::KeyMissing));
}

#[test]
fn explains_a_missing_data_directory() {
    let app = Core::start(
        None,
        Box::new(MemoryKeystore::new()),
        Arc::new(|| STEADY_MS),
        WAIT,
    );

    assert_eq!(app.state("").problem, Some(Problem::NoDataDir));
}

#[test]
fn lists_every_entry_with_its_note() {
    let dir = temp_dir();
    let app = keystore_core(dir.path(), STEADY_MS);

    let all = app.list("").unwrap();

    assert_eq!(all.len(), 3);
    assert_eq!(all[0].title, "Example Mail");
    assert_eq!(all[2].kind, EntryKind::Hotp);
    assert_eq!(app.list("cloud").unwrap().len(), 1);
}

#[test]
fn edits_the_name_and_the_note() {
    let dir = temp_dir();
    let mut app = keystore_core(dir.path(), STEADY_MS);
    let id = app.list("cloud").unwrap().remove(0).id;

    let edited = app
        .edit(
            &id,
            Some("Cloud 工作".to_owned()),
            Some("备用码在抽屉里".to_owned()),
        )
        .unwrap();

    assert_eq!(edited.title, "Cloud 工作");
    assert_eq!(edited.note, "备用码在抽屉里");
    assert_eq!(app.list("工作").unwrap()[0].id, id);
}

#[test]
fn refuses_bad_names_and_changes_nothing() {
    let dir = temp_dir();
    let mut app = keystore_core(dir.path(), STEADY_MS);
    let id = app.list("cloud").unwrap().remove(0).id;
    let long_note = "x".repeat(keep_vault::edit::NOTE_MAX_CHARS + 1);
    let long_title = "名".repeat(keep_vault::edit::TITLE_MAX_CHARS + 1);

    assert_eq!(
        app.edit(&id, Some("  ".to_owned()), None),
        Err(CoreError::Invalid("TITLE_EMPTY"))
    );
    assert_eq!(
        app.edit(&id, Some(long_title), None),
        Err(CoreError::Invalid("TITLE_TOO_LONG"))
    );
    assert_eq!(
        app.edit(&id, Some("New".to_owned()), Some(long_note)),
        Err(CoreError::Invalid("NOTE_TOO_LONG"))
    );
    assert_eq!(app.list("cloud").unwrap()[0].title, "Example Cloud");
    assert_eq!(
        app.edit("nope", None, Some(String::new())),
        Err(CoreError::NotFound)
    );
}

#[test]
fn editing_needs_the_password_first() {
    let dir = temp_dir();
    let mut app = password_core(dir.path());

    assert_eq!(app.list(""), Err(CoreError::NeedsPassword));
    assert_eq!(
        app.edit("x", None, Some(String::new())),
        Err(CoreError::NeedsPassword)
    );
}

#[test]
fn the_main_window_sees_every_entry_and_counter_codes_far_down_the_list() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir.path()), &keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || STEADY_MS);
    for byte in 0..25u8 {
        service
            .add(entry(
                &format!("Demo {byte}"),
                "Demo",
                byte + 10,
                CodeSpec::default(),
            ))
            .unwrap();
    }
    service
        .add(entry("Late counter", "Bank", 99, hotp()))
        .unwrap();
    service
        .add(entry("Steam", "Steam", 98, CodeSpec::steam(30).unwrap()))
        .unwrap();
    drop(service);
    let mut app = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(keystore),
        Arc::new(|| STEADY_MS),
        WAIT,
    );

    let all = app.state_all("");
    assert_eq!(all.entries.len(), 27);
    let steam = all.entries.iter().find(|e| e.title == "Steam").unwrap();
    assert_eq!(steam.kind, EntryKind::Steam);
    assert_eq!(steam.code.as_ref().map(String::len), Some(5));
    assert!(steam.valid_until_ms.is_some());

    let late = all
        .entries
        .iter()
        .find(|e| e.title == "Late counter")
        .unwrap()
        .id
        .clone();
    assert!(app.next_hotp(&late).unwrap().code.is_some());
}

#[test]
fn rows_carry_the_note() {
    let dir = temp_dir();
    let mut app = keystore_core(dir.path(), STEADY_MS);
    let id = app.list("cloud").unwrap().remove(0).id;
    app.edit(&id, None, Some("备用码在抽屉里".to_owned()))
        .unwrap();

    let row = app.state("cloud").entries.remove(0);

    assert_eq!(row.note, "备用码在抽屉里");
}

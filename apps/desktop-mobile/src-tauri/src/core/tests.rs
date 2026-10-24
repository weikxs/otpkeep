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
    assert_eq!(first.shortcut, Some(1));
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
    assert_eq!(app.state("").entries[10].shortcut, None);
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
    assert_eq!(app.state("").status, PanelStatus::Locked);
    assert!(app.state("").uses_master_password);
    assert_eq!(app.code("x"), Err(CoreError::Locked));

    assert_eq!(app.unlock("wrong").status, PanelStatus::Locked);
    let state = app.unlock(PASSWORD);

    assert_eq!(state.status, PanelStatus::Ready);
    assert_eq!(state.entries.len(), 3);
}

#[test]
fn locks_and_opens_again() {
    let dir = temp_dir();
    let mut app = password_core(dir.path());
    app.unlock(PASSWORD);

    app.lock();

    assert_eq!(app.state("").status, PanelStatus::Locked);
    assert_eq!(app.unlock(PASSWORD).status, PanelStatus::Ready);
}

#[test]
fn locking_a_keystore_vault_opens_it_again_without_a_password() {
    let dir = temp_dir();
    let mut app = keystore_core(dir.path(), STEADY_MS);

    app.lock();
    assert_eq!(app.state("").status, PanelStatus::Locked);

    assert_eq!(app.unlock("").status, PanelStatus::Ready);
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

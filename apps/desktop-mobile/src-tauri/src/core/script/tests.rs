use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use keep_passcode::{CodeKind, CodeSpec, Digest, SecretKey};
use keep_platform::MemoryKeystore;
use keep_platform::app_socket::Response;
use keep_vault::edit::NewEntry;
use keep_vault::open::{create_vault, create_with_master_password};
use keep_vault::sealed::KdfParams;
use keep_vault::service::VaultService;
use keep_vault::store::VaultPaths;

use crate::core::Core;

const WAIT: Duration = Duration::from_millis(200);
const NOW_MS: u64 = 1_700_000_020_000;

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("app-script-")
        .tempdir_in(std::env::temp_dir())
        .unwrap()
}

fn demo(title: &str, byte: u8, spec: CodeSpec) -> NewEntry {
    NewEntry {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "demo@example.com".to_owned(),
        secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
        spec,
        counter: 0,
        note: String::new(),
        tags: Vec::new(),
        keep_duplicate: true,
    }
}

fn core(dir: &Path) -> Core {
    let keystore = MemoryKeystore::new();
    let open = create_vault(&VaultPaths::new(dir), &keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || NOW_MS);
    service
        .add(demo("Example Cloud", 1, CodeSpec::default()))
        .unwrap();
    service
        .add(demo("Example Cloud Backup", 2, CodeSpec::default()))
        .unwrap();
    service
        .add(demo("Demo Git", 3, CodeSpec::default()))
        .unwrap();
    let hotp = CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap();
    service.add(demo("Example Bank", 4, hotp)).unwrap();
    drop(service);
    Core::start(
        Some(VaultPaths::new(dir)),
        Box::new(keystore),
        Arc::new(|| NOW_MS),
        WAIT,
    )
}

fn error_of(answer: &Response) -> &str {
    match answer {
        Response::Error(failure) => &failure.error,
        Response::Code(_) => "",
    }
}

#[test]
fn gives_the_code_of_the_one_matching_account() {
    let dir = temp_dir();
    let mut core = core(dir.path());

    let Response::Code(given) = core.script_code("git", false) else {
        panic!("no code");
    };

    assert_eq!(given.name, "Demo Git");
    assert_eq!(given.code.len(), 6);
    assert_eq!(given.valid_for_s, Some(20));
}

#[test]
fn an_exact_name_wins_among_several_matches() {
    let dir = temp_dir();
    let mut core = core(dir.path());

    let exact = core.script_code("example cloud", false);
    let several = core.script_code("example", false);

    assert!(matches!(exact, Response::Code(ref c) if c.name == "Example Cloud"));
    assert_eq!(error_of(&several), "AMBIGUOUS");
    let Response::Error(failure) = several else {
        panic!()
    };
    assert!(failure.candidates.len() >= 3);
}

#[test]
fn says_when_nothing_matches() {
    let dir = temp_dir();
    let mut core = core(dir.path());

    assert_eq!(
        error_of(&core.script_code("nothing like it", false)),
        "NOT_FOUND"
    );
}

#[test]
fn a_counter_account_needs_next_and_then_moves_on() {
    let dir = temp_dir();
    let mut core = core(dir.path());

    assert_eq!(error_of(&core.script_code("bank", false)), "NEEDS_NEXT");
    let Response::Code(first) = core.script_code("bank", true) else {
        panic!("no code")
    };
    let Response::Code(second) = core.script_code("bank", true) else {
        panic!("no code")
    };

    assert_ne!(first.code, second.code);
    assert_eq!(first.valid_for_s, None);
}

#[test]
fn a_locked_vault_gives_nothing() {
    let dir = temp_dir();
    let params = KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    drop(create_with_master_password(&VaultPaths::new(dir.path()), "demo", params, WAIT).unwrap());
    let mut core = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(|| NOW_MS),
        WAIT,
    );

    assert!(core.uses_master_password());
    assert_eq!(error_of(&core.script_code("git", false)), "LOCKED");
}

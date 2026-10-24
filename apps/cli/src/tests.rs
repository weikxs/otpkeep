use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_passcode::{CodeKind, CodeSpec, Digest, Moment, SecretKey, derive_passcode};
use keep_platform::MemoryKeystore;
use keep_vault::edit::NewEntry;
use keep_vault::open::{create_vault, create_with_master_password};
use keep_vault::sealed::KdfParams;
use keep_vault::service::VaultService;
use keep_vault::store::{VaultLock, VaultPaths};

use super::{
    Context, EXIT_AUTH_FAILED, EXIT_BUSY, EXIT_INVALID_INPUT, EXIT_MULTIPLE_MATCHES, EXIT_NO_MATCH,
    EXIT_OK, EXIT_READ_ONLY, EXIT_USAGE, EXIT_VAULT_ERROR, run,
};

const NOW_MS: u64 = 59_000;

#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature of Context::new_kdf"
)]
fn quick_kdf() -> Option<KdfParams> {
    Some(KdfParams {
        salt: [6; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    })
}
const WAIT: Duration = Duration::from_millis(200);

struct Outcome {
    code: u8,
    out: String,
    err: String,
}

fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("cli-").tempdir().unwrap()
}

fn entry(title: &str, issuer: &str, byte: u8, spec: CodeSpec, tags: &[&str]) -> NewEntry {
    // Only the first demo entry uses an example.com account, so searching "example" names one entry.
    let account = if byte == 1 {
        "alex@example.com"
    } else {
        "demo-user"
    };
    NewEntry {
        title: title.to_owned(),
        issuer: issuer.to_owned(),
        account: account.to_owned(),
        secret: SecretKey::from_bytes(vec![byte; 20]).unwrap(),
        spec,
        counter: 0,
        note: String::new(),
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        keep_duplicate: false,
    }
}

fn hotp() -> CodeSpec {
    CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, 6).unwrap()
}

/// A keystore vault in `dir` with three demo entries.
fn seed(dir: &Path, keystore: &MemoryKeystore) {
    let open = create_vault(&VaultPaths::new(dir), keystore, WAIT).unwrap();
    let mut service = VaultService::new(open, || NOW_MS);
    service
        .add(entry(
            "Example Cloud",
            "Example Cloud",
            1,
            CodeSpec::default(),
            &["work"],
        ))
        .unwrap();
    service
        .add(entry("Demo Git", "Demo Git", 2, CodeSpec::default(), &[]))
        .unwrap();
    service
        .add(entry(
            "Demo Git Staging",
            "Demo Git",
            3,
            CodeSpec::default(),
            &["test"],
        ))
        .unwrap();
    service
        .add(entry("Counter Demo", "Demo", 4, hotp(), &[]))
        .unwrap();
}

fn cli(dir: Option<&Path>, keystore: &MemoryKeystore, args: &[&str], stdin: &str) -> Outcome {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let mut input = stdin.as_bytes();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut context = Context {
        data_dir: dir.map(PathBuf::from),
        keystore,
        stdin: &mut input,
        stdin_is_tty: false,
        now_ms: || NOW_MS,
        prompt: &|_| None,
        new_kdf: quick_kdf,
        network: crate::lan::Network::Loopback(crate::lan::WindowDirectory::default()),
        lock_wait: WAIT,
    };
    let code = run(&args, &mut context, &mut out, &mut err);
    Outcome {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

fn expected_code(byte: u8) -> String {
    let secret = SecretKey::from_bytes(vec![byte; 20]).unwrap();
    derive_passcode(&secret, CodeSpec::default(), Moment::Time(59))
        .unwrap()
        .code()
        .to_owned()
}

#[test]
fn version_and_help_still_work() {
    let keystore = MemoryKeystore::new();

    assert_eq!(
        cli(None, &keystore, &["--version"], "").out,
        format!("otpkeep {}\n", env!("CARGO_PKG_VERSION"))
    );
    let help = cli(None, &keystore, &["help"], "");
    assert_eq!(help.code, EXIT_OK);
    assert!(help.out.contains("code <query>"), "{}", help.out);
    assert!(help.out.contains("list"), "{}", help.out);
}

#[test]
fn prints_the_code_of_a_unique_match() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let result = cli(Some(dir.path()), &keystore, &["code", "example"], "");

    assert_eq!(result.code, EXIT_OK, "{}", result.err);
    assert_eq!(result.out, format!("{}\n", expected_code(1)));
}

#[test]
fn several_matches_list_the_candidates_without_any_code() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let result = cli(Some(dir.path()), &keystore, &["code", "demo"], "");

    assert_eq!(result.code, EXIT_MULTIPLE_MATCHES);
    assert!(result.out.is_empty());
    assert!(result.err.contains("Demo Git Staging"), "{}", result.err);
    assert!(!result.err.contains(&expected_code(2)));
}

#[test]
fn an_exact_name_wins_among_several_matches() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let result = cli(Some(dir.path()), &keystore, &["code", "demo", "git"], "");

    assert_eq!(result.code, EXIT_OK, "{}", result.err);
    assert_eq!(result.out.trim(), expected_code(2));
}

#[test]
fn no_match_is_its_own_exit_code() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    assert_eq!(
        cli(Some(dir.path()), &keystore, &["code", "zzz"], "").code,
        EXIT_NO_MATCH
    );
}

#[test]
fn hotp_needs_next_and_next_advances_the_counter() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let without = cli(Some(dir.path()), &keystore, &["code", "counter"], "");
    let first = cli(
        Some(dir.path()),
        &keystore,
        &["code", "counter", "--next"],
        "",
    );
    let second = cli(
        Some(dir.path()),
        &keystore,
        &["code", "counter", "--next"],
        "",
    );

    assert_eq!(without.code, EXIT_INVALID_INPUT);
    assert!(without.err.contains("--next"), "{}", without.err);
    let secret = SecretKey::from_bytes(vec![4; 20]).unwrap();
    let code_at = |c| {
        derive_passcode(&secret, hotp(), Moment::Counter(c))
            .unwrap()
            .code()
            .to_owned()
    };
    assert_eq!(first.out.trim(), code_at(0));
    assert_eq!(second.out.trim(), code_at(1));
}

#[test]
fn json_output_has_a_stable_shape() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let code = cli(
        Some(dir.path()),
        &keystore,
        &["code", "example", "--json"],
        "",
    );
    let missing = cli(Some(dir.path()), &keystore, &["code", "zzz", "--json"], "");

    let value: serde_json::Value = serde_json::from_str(&code.out).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["code"], expected_code(1));
    assert_eq!(value["data"]["valid_for_s"], 1);
    let error: serde_json::Value = serde_json::from_str(&missing.out).unwrap();
    assert_eq!(
        (error["ok"].clone(), error["error"]["code"].clone()),
        (serde_json::json!(false), serde_json::json!("NO_MATCH"))
    );
}

#[test]
fn list_shows_names_and_never_codes_or_secrets() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);

    let all = cli(Some(dir.path()), &keystore, &["list"], "");
    let tagged = cli(
        Some(dir.path()),
        &keystore,
        &["list", "--tag", "WORK", "--json"],
        "",
    );

    assert_eq!(all.code, EXIT_OK);
    assert_eq!(all.out.lines().count(), 4);
    assert!(
        all.out
            .starts_with("Example Cloud\tExample Cloud\talex@example.com\n"),
        "{}",
        all.out
    );
    assert!(!all.out.contains(&expected_code(1)));
    let value: serde_json::Value = serde_json::from_str(&tagged.out).unwrap();
    assert_eq!(value["data"].as_array().unwrap().len(), 1);
    assert_eq!(value["data"][0]["title"], "Example Cloud");
}

#[test]
fn a_missing_vault_points_to_the_app() {
    let dir = temp_dir();

    let result = cli(Some(dir.path()), &MemoryKeystore::new(), &["list"], "");

    assert_eq!(result.code, EXIT_VAULT_ERROR);
    assert!(result.err.contains("app"), "{}", result.err);
}

#[test]
fn master_password_vaults_read_the_password_from_stdin_when_asked() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    let kdf = KdfParams {
        salt: [3; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let open =
        create_with_master_password(&VaultPaths::new(dir.path()), "demo password", kdf, WAIT)
            .unwrap();
    VaultService::new(open, || NOW_MS)
        .add(entry("Example Cloud", "", 1, CodeSpec::default(), &[]))
        .unwrap();

    let right = cli(
        Some(dir.path()),
        &keystore,
        &["code", "example", "--password-stdin"],
        "demo password\n",
    );
    let wrong = cli(
        Some(dir.path()),
        &keystore,
        &["code", "example", "--password-stdin"],
        "nope\n",
    );
    let not_asked = cli(Some(dir.path()), &keystore, &["code", "example"], "");

    assert_eq!(right.code, EXIT_OK, "{}", right.err);
    assert_eq!(right.out.trim(), expected_code(1));
    assert_eq!(wrong.code, EXIT_AUTH_FAILED);
    assert_eq!(not_asked.code, EXIT_AUTH_FAILED);
    assert!(
        not_asked.err.contains("--password-stdin"),
        "{}",
        not_asked.err
    );
}

#[test]
fn another_process_holding_the_vault_is_busy() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let _held = VaultLock::acquire(&VaultPaths::new(dir.path()), WAIT).unwrap();

    assert_eq!(
        cli(Some(dir.path()), &keystore, &["list"], "").code,
        EXIT_BUSY
    );
}

#[test]
fn a_read_only_vault_shows_codes_but_cannot_step_hotp() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    std::fs::write(dir.path().join("vault-v2.okv"), b"newer").unwrap();

    assert_eq!(
        cli(Some(dir.path()), &keystore, &["code", "example"], "").code,
        EXIT_OK
    );
    assert_eq!(
        cli(
            Some(dir.path()),
            &keystore,
            &["code", "counter", "--next"],
            ""
        )
        .code,
        EXIT_READ_ONLY
    );
}

#[test]
fn bad_command_lines_are_usage_errors() {
    let keystore = MemoryKeystore::new();
    for args in [
        &["code"][..],
        &["code", "x", "--bogus"],
        &["list", "--tag"],
        &["frobnicate"],
    ] {
        let result = cli(None, &keystore, args, "");
        assert_eq!(result.code, EXIT_USAGE, "{args:?}");
        assert!(
            result.err.contains("otpkeep help"),
            "{args:?}: {}",
            result.err
        );
    }
}

#[test]
fn every_kind_of_stopped_vault_says_what_to_do() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let paths = VaultPaths::new(dir.path());

    let key_missing = cli(Some(dir.path()), &MemoryKeystore::new(), &["list"], "");
    let unavailable = cli(
        Some(dir.path()),
        &MemoryKeystore::unavailable(),
        &["list"],
        "",
    );
    std::fs::write(paths.vault(), b"damaged").unwrap();
    let damaged = cli(Some(dir.path()), &keystore, &["list"], "");
    std::fs::remove_file(paths.vault()).unwrap();
    std::fs::remove_file(paths.prev()).unwrap();
    let data_missing = cli(Some(dir.path()), &keystore, &["list"], "");
    let no_dir = cli(None, &keystore, &["list"], "");

    for (result, words) in [
        (&key_missing, "key"),
        (&unavailable, "keystore"),
        (&damaged, "damaged"),
        (&data_missing, "missing"),
        (&no_dir, "data directory"),
    ] {
        assert_eq!(result.code, EXIT_VAULT_ERROR, "{}", result.err);
        assert!(result.err.contains(words), "{words}: {}", result.err);
    }
}

#[test]
fn a_terminal_prompts_for_the_master_password() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    let kdf = KdfParams {
        salt: [3; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let open =
        create_with_master_password(&VaultPaths::new(dir.path()), "typed", kdf, WAIT).unwrap();
    VaultService::new(open, || NOW_MS)
        .add(entry("Example Cloud", "", 1, CodeSpec::default(), &[]))
        .unwrap();
    let args: Vec<String> = ["code", "example"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut input: &[u8] = b"";
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut context = Context {
        data_dir: Some(dir.path().to_path_buf()),
        keystore: &keystore,
        stdin: &mut input,
        stdin_is_tty: true,
        now_ms: || NOW_MS,
        prompt: &|_| Some("typed".to_owned()),
        new_kdf: quick_kdf,
        network: crate::lan::Network::Loopback(crate::lan::WindowDirectory::default()),
        lock_wait: WAIT,
    };

    let code = run(&args, &mut context, &mut out, &mut err);

    assert_eq!(code, EXIT_OK, "{}", String::from_utf8_lossy(&err));
    assert_eq!(String::from_utf8(out).unwrap().trim(), expected_code(1));
}

mod transfer;

mod lan;

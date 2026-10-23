//! `otpkeep init`, `otpkeep import` and `otpkeep export`.

use std::path::Path;

use keep_platform::MemoryKeystore;
use keep_transfer::goldenpassport::read_secrets;

use super::{Outcome, WAIT, cli, quick_kdf, seed, temp_dir};
use crate::{
    Context, EXIT_CANCELLED, EXIT_INVALID_INPUT, EXIT_OK, EXIT_READ_ONLY, EXIT_USAGE,
    EXIT_VAULT_ERROR, Prompt, run,
};

const GP_EXPORT: &[u8] =
    include_bytes!("../../../../crates/keep-transfer/src/goldenpassport/fixtures/demo-0.1.secrets");

/// Runs on a terminal, answering prompts from `answers`.
fn on_terminal(
    dir: &Path,
    keystore: &MemoryKeystore,
    args: &[&str],
    answers: &dyn Fn(Prompt) -> Option<String>,
) -> Outcome {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let mut input: &[u8] = b"";
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut context = Context {
        data_dir: Some(dir.to_path_buf()),
        keystore,
        stdin: &mut input,
        stdin_is_tty: true,
        now_ms: || super::NOW_MS,
        prompt: answers,
        new_kdf: quick_kdf,
        lock_wait: WAIT,
    };
    let code = run(&args, &mut context, &mut out, &mut err);
    Outcome {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

fn names(dir: &Path, keystore: &MemoryKeystore) -> Vec<String> {
    let listed = cli(Some(dir), keystore, &["list"], "");
    assert_eq!(listed.code, EXIT_OK, "{}", listed.err);
    listed
        .out
        .lines()
        .map(|l| l.split('\t').next().unwrap().to_owned())
        .collect()
}

fn file(dir: &Path, name: &str, contents: &[u8]) -> String {
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn init_creates_a_vault_once() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();

    let first = cli(Some(dir.path()), &keystore, &["init"], "");
    let again = cli(Some(dir.path()), &keystore, &["init"], "");

    assert_eq!(first.code, EXIT_OK, "{}", first.err);
    assert!(names(dir.path(), &keystore).is_empty());
    assert_eq!(again.code, EXIT_VAULT_ERROR);
    assert!(again.err.contains("already"), "{}", again.err);
}

#[test]
fn init_with_a_master_password_and_without_a_keystore() {
    let dir = temp_dir();
    let none = MemoryKeystore::unavailable();

    let refused = cli(Some(dir.path()), &none, &["init"], "");
    let created = cli(
        Some(dir.path()),
        &none,
        &["init", "--master-password", "--password-stdin"],
        "pw\n",
    );
    let listed = cli(
        Some(dir.path()),
        &none,
        &["list", "--password-stdin"],
        "pw\n",
    );

    assert_eq!(refused.code, EXIT_VAULT_ERROR);
    assert!(refused.err.contains("--master-password"), "{}", refused.err);
    assert_eq!(created.code, EXIT_OK, "{}", created.err);
    assert_eq!(listed.code, EXIT_OK, "{}", listed.err);
}

#[test]
fn imports_a_goldenpassport_export_and_reports_what_it_skipped() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    cli(Some(dir.path()), &keystore, &["init"], "");
    let path = file(dir.path(), "GoldenPassport.secrets", GP_EXPORT);

    let first = cli(Some(dir.path()), &keystore, &["import", &path, "--yes"], "");
    let again = cli(Some(dir.path()), &keystore, &["import", &path, "--yes"], "");

    assert_eq!(first.code, EXIT_OK, "{}", first.err);
    assert!(first.out.contains("3 new"), "{}", first.out);
    assert!(first.err.contains("Broken"), "{}", first.err);
    assert_eq!(
        names(dir.path(), &keystore),
        ["Demo Git", "Example Cloud", "示例银行 个人"]
    );
    assert_eq!(again.code, EXIT_OK);
    assert!(
        again.out.contains("0 new") && again.out.contains("3 already"),
        "{}",
        again.out
    );
}

#[test]
fn import_asks_before_adding_and_can_be_declined() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    cli(Some(dir.path()), &keystore, &["init"], "");
    let path = file(dir.path(), "GoldenPassport.secrets", GP_EXPORT);

    let not_on_terminal = cli(Some(dir.path()), &keystore, &["import", &path], "");
    let declined = on_terminal(dir.path(), &keystore, &["import", &path], &|_| {
        Some("n".into())
    });

    assert_eq!(not_on_terminal.code, EXIT_CANCELLED);
    assert!(
        not_on_terminal.err.contains("--yes"),
        "{}",
        not_on_terminal.err
    );
    assert_eq!(declined.code, EXIT_CANCELLED);
    assert!(names(dir.path(), &keystore).is_empty());
    let accepted = on_terminal(dir.path(), &keystore, &["import", &path], &|_| {
        Some("y".into())
    });
    assert_eq!(accepted.code, EXIT_OK, "{}", accepted.err);
    assert_eq!(names(dir.path(), &keystore).len(), 3);
}

#[test]
fn imports_link_sheets_goldenpassport_json_and_google_links() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    cli(Some(dir.path()), &keystore, &["init"], "");
    let sheet = file(
        dir.path(),
        "links.txt",
        b"# Sheet entry\notpauth://totp/Sheet?secret=GEZDGNBVGY3TQOJQ\n",
    );
    let json = file(
        dir.path(),
        "accounts.json",
        br#"[{"id":"x","name":"From JSON","url":"otpauth://totp/J?secret=JBSWY3DPEHPK3PXP"}]"#,
    );

    for path in [&sheet, &json] {
        let result = cli(Some(dir.path()), &keystore, &["import", path, "--yes"], "");
        assert_eq!(result.code, EXIT_OK, "{path}: {}", result.err);
    }
    assert_eq!(names(dir.path(), &keystore), ["Sheet entry", "From JSON"]);
}

#[test]
fn rejects_files_it_cannot_read() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    cli(Some(dir.path()), &keystore, &["init"], "");
    let junk = file(dir.path(), "junk.bin", &[0xff, 0x00, 0x13]);

    let unknown = cli(Some(dir.path()), &keystore, &["import", &junk, "--yes"], "");
    let missing = cli(
        Some(dir.path()),
        &keystore,
        &["import", "/tmp/otpkeep-test/does-not-exist", "--yes"],
        "",
    );
    let no_path = cli(Some(dir.path()), &keystore, &["import"], "");

    assert_eq!(unknown.code, EXIT_INVALID_INPUT, "{}", unknown.err);
    assert_eq!(missing.code, EXIT_INVALID_INPUT, "{}", missing.err);
    assert_eq!(no_path.code, EXIT_USAGE);
}

#[test]
fn a_backup_restores_into_another_vault() {
    let (source, target) = (temp_dir(), temp_dir());
    let keystore = MemoryKeystore::new();
    seed(source.path(), &keystore);
    let other_keystore = MemoryKeystore::new();
    cli(Some(target.path()), &other_keystore, &["init"], "");
    let password = file(source.path(), "pw.txt", b"backup demo password\n");
    let backup = source
        .path()
        .join("vault.otpkeep")
        .to_string_lossy()
        .into_owned();

    let exported = cli(
        Some(source.path()),
        &keystore,
        &[
            "export",
            "--format",
            "backup",
            "--out",
            &backup,
            "--backup-password-file",
            &password,
        ],
        "",
    );
    let imported = cli(
        Some(target.path()),
        &other_keystore,
        &[
            "import",
            &backup,
            "--backup-password-file",
            &password,
            "--yes",
        ],
        "",
    );
    let wrong = file(source.path(), "wrong.txt", b"wrong\n");
    let refused = cli(
        Some(target.path()),
        &other_keystore,
        &["import", &backup, "--backup-password-file", &wrong, "--yes"],
        "",
    );

    assert_eq!(exported.code, EXIT_OK, "{}", exported.err);
    assert_eq!(imported.code, EXIT_OK, "{}", imported.err);
    assert_eq!(
        names(target.path(), &other_keystore),
        names(source.path(), &keystore)
    );
    assert_eq!(refused.code, super::EXIT_AUTH_FAILED);
}

#[test]
fn plain_text_exports_need_an_explicit_flag_and_a_confirmation() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let out = dir.path().join("links.txt").to_string_lossy().into_owned();

    let without_flag = cli(
        Some(dir.path()),
        &keystore,
        &["export", "--format", "uris", "--out", &out],
        "",
    );
    let unconfirmed = cli(
        Some(dir.path()),
        &keystore,
        &["export", "--format", "uris", "--out", &out, "--plaintext"],
        "",
    );
    let done = cli(
        Some(dir.path()),
        &keystore,
        &[
            "export",
            "--format",
            "uris",
            "--out",
            &out,
            "--plaintext",
            "--yes",
        ],
        "",
    );

    assert_eq!(without_flag.code, EXIT_USAGE);
    assert!(
        without_flag.err.contains("--plaintext"),
        "{}",
        without_flag.err
    );
    assert_eq!(unconfirmed.code, EXIT_CANCELLED);
    assert_eq!(done.code, EXIT_OK, "{}", done.err);
    let sheet = std::fs::read_to_string(&out).unwrap();
    assert_eq!(keep_keyuri::read_sheet(&sheet).items.len(), 4);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn exports_for_goldenpassport_without_what_it_would_compute_wrongly() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let out = dir
        .path()
        .join("OTPKeep.secrets")
        .to_string_lossy()
        .into_owned();

    let result = cli(
        Some(dir.path()),
        &keystore,
        &[
            "export",
            "--format",
            "goldenpassport",
            "--out",
            &out,
            "--plaintext",
            "--yes",
        ],
        "",
    );

    assert_eq!(result.code, EXIT_OK, "{}", result.err);
    assert!(
        result.err.contains("Counter Demo"),
        "the HOTP entry is named as left out: {}",
        result.err
    );
    assert_eq!(
        read_secrets(&std::fs::read(&out).unwrap())
            .unwrap()
            .candidates
            .len(),
        3
    );
}

#[test]
fn exports_google_transfer_codes_as_svg_files() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let out = dir.path().join("google");

    let result = cli(
        Some(dir.path()),
        &keystore,
        &[
            "export",
            "--format",
            "google",
            "--out",
            out.to_str().unwrap(),
            "--plaintext",
            "--yes",
        ],
        "",
    );

    assert_eq!(result.code, EXIT_OK, "{}", result.err);
    let svgs: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(svgs.len(), 1, "{svgs:?}");
    assert!(
        std::fs::read_to_string(out.join("google-1-of-1.svg"))
            .unwrap()
            .contains("<svg")
    );
}

#[test]
fn never_overwrites_an_existing_file() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    let out = file(dir.path(), "existing.txt", b"keep me");

    let result = cli(
        Some(dir.path()),
        &keystore,
        &[
            "export",
            "--format",
            "uris",
            "--out",
            &out,
            "--plaintext",
            "--yes",
        ],
        "",
    );

    assert_eq!(result.code, EXIT_INVALID_INPUT);
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
}

#[test]
fn a_read_only_vault_cannot_import() {
    let dir = temp_dir();
    let keystore = MemoryKeystore::new();
    seed(dir.path(), &keystore);
    std::fs::write(dir.path().join("vault-v2.okv"), b"newer").unwrap();
    let path = file(dir.path(), "GoldenPassport.secrets", GP_EXPORT);

    assert_eq!(
        cli(Some(dir.path()), &keystore, &["import", &path, "--yes"], "").code,
        EXIT_READ_ONLY
    );
}

//! Runs the built `otpkeep` binary the way a user would.
#![allow(clippy::unwrap_used, reason = "a test harness")]

use std::process::Command;

fn otpkeep(args: &[&str]) -> std::process::Output {
    let result = Command::new(env!("CARGO_BIN_EXE_otpkeep"))
        .args(args)
        .output();
    match result {
        Ok(output) => output,
        Err(error) => unreachable!("the test binary could not start: {error}"),
    }
}

#[test]
fn version_exits_zero_and_prints_to_stdout() {
    let output = otpkeep(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("otpkeep {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn an_unknown_command_exits_with_the_usage_code() {
    let output = otpkeep(&["frobnicate"]);

    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}

mod with_a_vault {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use keep_passcode::{CodeSpec, SecretKey};
    use keep_vault::edit::NewEntry;
    use keep_vault::open::create_with_master_password;
    use keep_vault::sealed::KdfParams;
    use keep_vault::service::VaultService;
    use keep_vault::store::{VaultLock, VaultPaths};

    const PASSWORD: &str = "demo password";

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix("cli-bin-")
            .tempdir()
            .unwrap();
        let kdf = KdfParams {
            salt: [3; 16],
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        };
        let open = create_with_master_password(
            &VaultPaths::new(dir.path()),
            PASSWORD,
            kdf,
            Duration::from_secs(1),
        )
        .unwrap();
        let mut service = VaultService::new(open, || 1_000);
        service
            .add(NewEntry {
                title: "Example Cloud".into(),
                issuer: "Example Cloud".into(),
                account: "alex@example.com".into(),
                secret: SecretKey::from_bytes(vec![1; 20]).unwrap(),
                spec: CodeSpec::default(),
                counter: 0,
                note: String::new(),
                tags: vec![],
                keep_duplicate: false,
            })
            .unwrap();
        dir
    }

    fn otpkeep(dir: &std::path::Path, args: &[&str], stdin: &str) -> std::process::Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_otpkeep"))
            .args(args)
            .env("OTPKEEP_DATA_DIR", dir)
            .env("OTPKEEP_KEYSTORE", "memory")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    #[test]
    fn prints_a_six_digit_code_from_a_master_password_vault() {
        let dir = vault();

        let output = otpkeep(
            dir.path(),
            &["code", "example", "--password-stdin"],
            &format!("{PASSWORD}\n"),
        );

        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let code = String::from_utf8(output.stdout).unwrap();
        assert_eq!(code.trim().len(), 6);
        assert!(code.trim().bytes().all(|b| b.is_ascii_digit()));
    }

    #[test]
    fn another_process_holding_the_lock_makes_the_command_wait_then_report_busy() {
        let dir = vault();
        let _held =
            VaultLock::acquire(&VaultPaths::new(dir.path()), Duration::from_secs(1)).unwrap();

        let output = otpkeep(
            dir.path(),
            &["list", "--password-stdin"],
            &format!("{PASSWORD}\n"),
        );

        assert_eq!(
            output.status.code(),
            Some(6),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("another OTPKeep process"));
    }

    #[test]
    fn an_empty_data_directory_has_no_vault() {
        let dir = tempfile::Builder::new()
            .prefix("cli-bin-")
            .tempdir()
            .unwrap();

        let output = otpkeep(dir.path(), &["list"], "");

        assert_eq!(output.status.code(), Some(7));
        assert!(output.stdout.is_empty());
    }
}

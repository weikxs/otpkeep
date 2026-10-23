//! M1 acceptance (docs: 02-plan, M1): import a demo link sheet, then `otpkeep list` and `otpkeep code`, as a user
//! would, against a real vault on disk.
#![allow(clippy::unwrap_used, reason = "a test harness")]

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keep_passcode::{CodeSpec, Moment, SecretKey, base32, derive_passcode};
use keep_transfer::import::{Choice, commit_import, from_sheet, preview};
use keep_vault::open::create_with_master_password;
use keep_vault::sealed::KdfParams;
use keep_vault::service::{VaultService, system_now_ms};
use keep_vault::store::VaultPaths;

const PASSWORD: &str = "demo password";

fn otpkeep(dir: &std::path::Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_otpkeep"))
        .args(args)
        .arg("--password-stdin")
        .env("OTPKEEP_DATA_DIR", dir)
        .env("OTPKEEP_KEYSTORE", "memory")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{PASSWORD}").unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn import_a_link_sheet_then_list_and_print_a_code() {
    let dir = tempfile::Builder::new()
        .prefix("acceptance-")
        .tempdir()
        .unwrap();
    let kdf = KdfParams {
        salt: [2; 16],
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
    let mut service = VaultService::new(open, system_now_ms);
    let demo = base32::encode(b"12345678901234567890").to_string();
    let sheet = format!(
        "# Demo\n# note: the demo account\notpauth://totp/Demo:alex@example.com?secret={demo}&issuer=Demo\n\n\
         # Example Cloud\notpauth://totp/Example%20Cloud:alex?secret=JBSWY3DPEHPK3PXP&issuer=Example%20Cloud\n"
    );
    let read = keep_keyuri::read_sheet(&sheet);
    let p = preview(from_sheet(&read), service.doc());
    let (doc, added) = commit_import(service.doc(), &p, &Choice::all(&p), system_now_ms()).unwrap();
    service.commit_derived(doc).unwrap();
    drop(service);
    assert_eq!(added.len(), 2);

    let list = otpkeep(dir.path(), &["list"]);
    let code = otpkeep(dir.path(), &["code", "demo"]);

    assert_eq!(
        list.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert_eq!(
        String::from_utf8(list.stdout).unwrap(),
        "Demo\tDemo\talex@example.com\nExample Cloud\tExample Cloud\talex\n"
    );
    assert_eq!(
        code.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&code.stderr)
    );
    let printed = String::from_utf8(code.stdout).unwrap().trim().to_owned();
    let secret = SecretKey::from_bytes(b"12345678901234567890".to_vec()).unwrap();
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    let around: Vec<String> = [now - 30, now, now + 30]
        .iter()
        .map(|t| {
            derive_passcode(&secret, CodeSpec::default(), Moment::Time(*t))
                .unwrap()
                .code()
                .to_owned()
        })
        .collect();
    assert!(around.contains(&printed), "{printed} not in {around:?}");
}

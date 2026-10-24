//! Steam Guard end to end, as a user would: import links in the three common forms, print a code, export.
#![allow(clippy::unwrap_used, reason = "a test harness")]

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keep_passcode::{CodeSpec, Moment, SecretKey, base32, derive_passcode};
use keep_vault::open::create_with_master_password;
use keep_vault::sealed::KdfParams;
use keep_vault::store::VaultPaths;

const PASSWORD: &str = "demo password";
const STEAM_ALPHABET: &str = "23456789BCDFGHJKMNPQRTVWXY";

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

fn ok(output: &Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn steam_guard_links_import_print_steam_codes_and_export_as_steam_links() {
    let dir = tempfile::Builder::new().prefix("steam-").tempdir().unwrap();
    let kdf = KdfParams {
        salt: [3; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    drop(
        create_with_master_password(
            &VaultPaths::new(dir.path()),
            PASSWORD,
            kdf,
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let secrets = [
        b"steam-demo-secret-01",
        b"steam-demo-secret-02",
        b"steam-demo-secret-03",
    ];
    let [a, b, c] = secrets.map(|s| base32::encode(s).to_string());
    let sheet = dir.path().join("links.txt");
    std::fs::write(
        &sheet,
        format!(
            "otpauth://steam/Steam:first?secret={a}\n\
             otpauth://totp/Steam:second?secret={b}&digits=5&issuer=Steam&encoder=steam\n\
             otpauth://totp/Steam:third?secret={c}&issuer=Steam\n"
        ),
    )
    .unwrap();

    ok(&otpkeep(
        dir.path(),
        &["import", sheet.to_str().unwrap(), "--yes"],
    ));
    let printed = ok(&otpkeep(dir.path(), &["code", "second"]))
        .trim()
        .to_owned();

    assert_eq!(printed.len(), 5);
    assert!(
        printed.chars().all(|ch| STEAM_ALPHABET.contains(ch)),
        "{printed}"
    );
    let secret = SecretKey::from_bytes(secrets[1].to_vec()).unwrap();
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
            derive_passcode(&secret, CodeSpec::steam(30).unwrap(), Moment::Time(*t))
                .unwrap()
                .code()
                .to_owned()
        })
        .collect();
    assert!(around.contains(&printed), "{printed} not in {around:?}");

    let out = dir.path().join("export.txt");
    ok(&otpkeep(
        dir.path(),
        &[
            "export",
            "--format",
            "uris",
            "--plaintext",
            "--yes",
            "--out",
            out.to_str().unwrap(),
        ],
    ));
    let exported = std::fs::read_to_string(&out).unwrap();
    assert_eq!(
        exported.matches("otpauth://steam/").count(),
        3,
        "{exported}"
    );
}

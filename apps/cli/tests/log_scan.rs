//! PRD FR-9.5: logs never contain secrets, codes, names, notes, tags, accounts, search terms or passwords.
//! A whole scenario runs with every log level captured, then the output is searched for each of them.
#![allow(clippy::unwrap_used, clippy::panic, reason = "a test harness")]

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use keep_find::{FindContext, FindIndex};
use keep_passcode::{CodeSpec, SecretKey, base32};
use keep_platform::MemoryKeystore;
use keep_transfer::backup::{read_backup, write_backup};
use keep_transfer::import::{Choice, commit_import, from_sheet, preview};
use keep_vault::edit::{Change, NewEntry};
use keep_vault::open::{Startup, create_vault, start};
use keep_vault::sealed::KdfParams;
use keep_vault::service::VaultService;
use keep_vault::store::VaultPaths;

const TITLE: &str = "LOGSCAN-TITLE Bank";
const ISSUER: &str = "LOGSCAN-ISSUER";
const ACCOUNT: &str = "logscan-account@example.com";
const NOTE: &str = "LOGSCAN-NOTE recovery codes";
const TAG: &str = "LOGSCAN-TAG";
const QUERY: &str = "LOGSCAN-QUERY";
const PASSWORD: &str = "LOGSCAN-PASSWORD";
const SECRET: &[u8] = b"LOGSCAN-SECRET-BYTES";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn scenario() {
    let dir = tempfile::Builder::new()
        .prefix("logscan-")
        .tempdir()
        .unwrap();
    let paths = VaultPaths::new(dir.path());
    let keystore = MemoryKeystore::new();
    let wait = Duration::from_millis(200);
    let mut service = VaultService::new(create_vault(&paths, &keystore, wait).unwrap(), || 59_000);
    let id = service
        .add(NewEntry {
            title: TITLE.into(),
            issuer: ISSUER.into(),
            account: ACCOUNT.into(),
            secret: SecretKey::from_bytes(SECRET.to_vec()).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            note: NOTE.into(),
            tags: vec![TAG.into()],
            keep_duplicate: false,
        })
        .unwrap();
    service
        .edit(id, Change::Title(format!("{TITLE} renamed")))
        .unwrap();
    let code = service.passcode(id).unwrap();
    let entries = service.ordered();
    let _ = FindIndex::build(&entries).find(QUERY, &FindContext::default());
    let kdf = KdfParams {
        salt: [1; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let backup = write_backup(&service.ordered(), PASSWORD, kdf).unwrap();
    let _ = read_backup(&backup, PASSWORD).unwrap();
    let _ = read_backup(&backup, "wrong password");
    let link = format!(
        "otpauth://totp/Other?secret={}",
        &*base32::encode(b"LOGSCAN-OTHER-SECRET")
    );
    let candidates = from_sheet(&keep_keyuri::read_sheet(&link));
    let doc = service.doc().clone();
    let p = preview(candidates, &doc);
    let _ = commit_import(&doc, &p, &Choice::all(&p), 60_000).unwrap();
    service.trash(id).unwrap();
    drop(service);
    match start(&paths, &keystore, wait) {
        Startup::Ready(_) => {}
        other => panic!("{other:?}"),
    }
    std::fs::write(paths.local(), b"damaged").unwrap();
    if let Startup::Ready(open) = start(&paths, &keystore, wait) {
        let _ = VaultService::new(open, || 0);
    }
    assert_eq!(code.code().len(), 6);
    CODE.with(|c| code.code().clone_into(&mut c.borrow_mut()));
}

thread_local! {
    static CODE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[test]
fn logs_hold_no_secrets_or_personal_data() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, scenario);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    let code = CODE.with(|c| c.borrow().clone());

    for event in [
        "vault opened",
        "vault saved",
        "import committed",
        "history file damaged",
    ] {
        assert!(log.contains(event), "expected a '{event}' event in:\n{log}");
    }
    let secret_b32 = base32::encode(SECRET);
    let forbidden = [
        TITLE,
        ISSUER,
        ACCOUNT,
        NOTE,
        TAG,
        QUERY,
        PASSWORD,
        "LOGSCAN-SECRET",
        "LOGSCAN-OTHER",
        &secret_b32,
        &code,
    ];
    for needle in forbidden {
        assert!(!log.contains(needle), "the log contains '{needle}':\n{log}");
    }
}

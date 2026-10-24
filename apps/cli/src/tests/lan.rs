//! Pairing and sync between two devices in one process, over loopback.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use keep_platform::MemoryKeystore;
use keep_vault::service::system_now_ms;

use super::{Outcome, WAIT, quick_kdf, seed, temp_dir};
use crate::lan::{Network, WindowDirectory};
use crate::{Context, EXIT_CANCELLED, EXIT_NOT_SYNCED, EXIT_OK, Prompt, run};

/// Output another thread can read while the command runs.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Shared {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }

    /// Waits for a line `label` followed by digits, and returns the digits.
    fn code_after(&self, label: &str) -> String {
        for _ in 0..400 {
            if let Some(rest) = self.text().split(label).nth(1) {
                let line = rest.lines().next().unwrap_or_default();
                let digits: String = line
                    .split('(')
                    .next()
                    .unwrap()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                if !digits.is_empty() {
                    return digits;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("no '{label}' in: {}", self.text());
    }
}

/// Runs `args` as a device in `dir`, answering prompts with `answer`, writing output to `out`.
fn device(
    dir: &Path,
    keystore: &MemoryKeystore,
    directory: &WindowDirectory,
    args: &[&str],
    answer: &(dyn Fn(Prompt) -> Option<String> + Sync),
    out: &Shared,
    tty: bool,
) -> Outcome {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let mut input: &[u8] = b"";
    let mut err = Vec::new();
    let mut context = Context {
        data_dir: Some(dir.to_path_buf()),
        keystore,
        stdin: &mut input,
        stdin_is_tty: tty,
        now_ms: system_now_ms,
        prompt: answer,
        new_kdf: quick_kdf,
        lock_wait: WAIT,
        network: Network::Loopback(directory.clone()),
    };
    let mut writer = out.clone();
    let code = run(&args, &mut context, &mut writer, &mut err);
    Outcome {
        code,
        out: out.text(),
        err: String::from_utf8(err).unwrap(),
    }
}

#[allow(clippy::needless_pass_by_value, reason = "the signature of a prompt")]
fn yes(prompt: Prompt) -> Option<String> {
    match prompt {
        Prompt::Confirm(_) => Some("y".to_owned()),
        _ => None,
    }
}

struct Device {
    dir: tempfile::TempDir,
    keystore: MemoryKeystore,
}

impl Device {
    fn new(seeded: bool) -> Self {
        let dir = temp_dir();
        let keystore = MemoryKeystore::new();
        if seeded {
            seed(dir.path(), &keystore);
        } else {
            let created = super::cli(Some(dir.path()), &keystore, &["init"], "");
            assert_eq!(created.code, EXIT_OK, "{}", created.err);
        }
        Self { dir, keystore }
    }

    fn run(
        &self,
        directory: &WindowDirectory,
        args: &[&str],
        answer: &(dyn Fn(Prompt) -> Option<String> + Sync),
        out: &Shared,
    ) -> Outcome {
        device(
            self.dir.path(),
            &self.keystore,
            directory,
            args,
            answer,
            out,
            true,
        )
    }

    fn list(&self) -> Vec<String> {
        let listed = super::cli(Some(self.dir.path()), &self.keystore, &["list"], "");
        listed
            .out
            .lines()
            .map(|l| l.split('\t').next().unwrap().to_owned())
            .collect()
    }
}

/// Device `a` opens with `open_args`, device `b` joins with `join_args`, typing the code `a` shows after `label`.
fn session(
    a: &Device,
    b: &Device,
    open_args: &[&str],
    join_args: &[&str],
    label: &str,
    a_answer: &(dyn Fn(Prompt) -> Option<String> + Sync),
) -> (Outcome, Outcome) {
    let directory = WindowDirectory::default();
    let a_out = Shared::default();
    let b_out = Shared::default();
    std::thread::scope(|scope| {
        let opener = scope.spawn(|| a.run(&directory, open_args, a_answer, &a_out));
        let a_out_for_b = a_out.clone();
        let label = label.to_owned();
        let b_answer = move |prompt: Prompt| match prompt {
            Prompt::Text(_) => Some(a_out_for_b.code_after(&label)),
            Prompt::Confirm(_) => Some("y".to_owned()),
            _ => None,
        };
        let joined = b.run(&directory, join_args, &b_answer, &b_out);
        (opener.join().unwrap(), joined)
    })
}

#[test]
fn two_devices_pair_and_then_sync_from_the_command_line() {
    let mac = Device::new(true);
    let phone = Device::new(false);

    let (opened, joined) = session(
        &mac,
        &phone,
        &["pair", "open", "--name", "Study Mac"],
        &["pair", "join", "--name", "Phone"],
        "Pairing code:",
        &yes,
    );

    assert_eq!(opened.code, EXIT_OK, "{}{}", opened.out, opened.err);
    assert_eq!(joined.code, EXIT_OK, "{}{}", joined.out, joined.err);
    assert!(
        opened.out.contains("Started a new sync group"),
        "{}",
        opened.out
    );
    assert_eq!(phone.list().len(), 4, "the phone has the Mac's entries");
    let devices = super::cli(Some(mac.dir.path()), &mac.keystore, &["devices"], "");
    assert!(
        devices.out.contains("Study Mac") && devices.out.contains("Phone"),
        "{}",
        devices.out
    );

    let sheet = phone.dir.path().join("new.txt");
    std::fs::write(
        &sheet,
        "# Added on the phone\notpauth://totp/Phone?secret=GEZDGNBVGY3TQOJQ\n",
    )
    .unwrap();
    let added = super::cli(
        Some(phone.dir.path()),
        &phone.keystore,
        &["import", sheet.to_str().unwrap(), "--yes"],
        "",
    );
    assert_eq!(added.code, EXIT_OK, "{}", added.err);
    let (opened, joined) = session(
        &mac,
        &phone,
        &["sync", "wait"],
        &["sync", "join"],
        "Sync code:",
        &yes,
    );

    assert_eq!(opened.code, EXIT_OK, "{}{}", opened.out, opened.err);
    assert_eq!(joined.code, EXIT_OK, "{}{}", joined.out, joined.err);
    assert!(
        joined.out.contains("Synced with Study Mac"),
        "{}",
        joined.out
    );
    assert!(
        mac.list().contains(&"Added on the phone".to_owned()),
        "{:?}",
        mac.list()
    );
}

#[test]
fn a_declined_sync_request_changes_nothing() {
    let mac = Device::new(true);
    let phone = Device::new(false);
    let (opened, joined) = session(
        &mac,
        &phone,
        &["pair", "open"],
        &["pair", "join"],
        "Pairing code:",
        &yes,
    );
    assert_eq!(
        (opened.code, joined.code),
        (EXIT_OK, EXIT_OK),
        "{}{}",
        opened.err,
        joined.err
    );
    let no = |prompt: Prompt| match prompt {
        Prompt::Confirm(_) => Some("n".to_owned()),
        _ => None,
    };

    let (opened, joined) = session(
        &mac,
        &phone,
        &["sync", "wait"],
        &["sync", "join", "--json"],
        "Sync code:",
        &no,
    );

    assert_eq!(opened.code, EXIT_NOT_SYNCED);
    assert_eq!(joined.code, EXIT_NOT_SYNCED);
    assert!(joined.out.contains("\"REJECTED\""), "{}", joined.out);
}

#[test]
fn sessions_need_a_person_at_a_terminal() {
    let mac = Device::new(true);
    let directory = WindowDirectory::default();

    let result = device(
        mac.dir.path(),
        &mac.keystore,
        &directory,
        &["pair", "open"],
        &yes,
        &Shared::default(),
        false,
    );

    assert_eq!(result.code, EXIT_CANCELLED);
    assert!(result.err.contains("terminal"), "{}", result.err);
}

#[test]
fn a_device_already_in_a_group_cannot_pair_again_and_can_leave() {
    let mac = Device::new(true);
    let phone = Device::new(false);
    let (opened, joined) = session(
        &mac,
        &phone,
        &["pair", "open"],
        &["pair", "join"],
        "Pairing code:",
        &yes,
    );
    assert_eq!((opened.code, joined.code), (EXIT_OK, EXIT_OK));

    let again = phone.run(
        &WindowDirectory::default(),
        &["pair", "join", "--json"],
        &yes,
        &Shared::default(),
    );
    assert_eq!(again.code, EXIT_NOT_SYNCED);
    assert!(again.out.contains("OTHER_GROUP"), "{}", again.out);

    let renamed = phone.run(
        &WindowDirectory::default(),
        &["rename", "Old phone"],
        &yes,
        &Shared::default(),
    );
    assert_eq!(renamed.code, EXIT_OK, "{}", renamed.err);
    let left = phone.run(
        &WindowDirectory::default(),
        &["group", "leave"],
        &yes,
        &Shared::default(),
    );
    assert_eq!(left.code, EXIT_OK, "{}", left.err);
    assert!(left.out.contains("Old phone"), "{}", left.out);
    assert_eq!(phone.list().len(), 4, "the entries stay");
    let gone = super::cli(Some(phone.dir.path()), &phone.keystore, &["devices"], "");
    assert_eq!(gone.code, EXIT_NOT_SYNCED);
}

#[test]
fn a_removed_device_leaves_the_device_list() {
    let mac = Device::new(true);
    let phone = Device::new(false);
    let (opened, joined) = session(
        &mac,
        &phone,
        &["pair", "open"],
        &["pair", "join", "--name", "Lost phone"],
        "Pairing code:",
        &yes,
    );
    assert_eq!((opened.code, joined.code), (EXIT_OK, EXIT_OK));
    let listed = super::cli(
        Some(mac.dir.path()),
        &mac.keystore,
        &["devices", "--json"],
        "",
    );
    let rows: serde_json::Value = serde_json::from_str(listed.out.trim()).unwrap();
    let fingerprint = rows["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Lost phone")
        .unwrap()["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();

    let removed = mac.run(
        &WindowDirectory::default(),
        &["devices", "remove", &fingerprint[..9]],
        &yes,
        &Shared::default(),
    );
    assert_eq!(removed.code, EXIT_OK, "{}", removed.err);
    let after = super::cli(Some(mac.dir.path()), &mac.keystore, &["devices"], "");
    assert!(!after.out.contains("Lost phone"), "{}", after.out);
}

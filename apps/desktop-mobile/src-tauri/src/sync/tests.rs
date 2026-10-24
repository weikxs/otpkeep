//! Two devices in one process, on loopback: pairing, syncing, declining, wrong codes and closing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keep_link::{Network, WindowDirectory};
use keep_platform::MemoryKeystore;
use keep_transfer::import::Choice;
use keep_vault::store::VaultPaths;

use super::join::{Looking, PairWith};
use super::{AskKind, Notify, SharedCore, SyncView, Syncer};
use crate::core::Core;

const WAIT: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(20);
const PATIENCE: Duration = Duration::from_secs(20);

#[derive(Default)]
struct Seen {
    views: Mutex<Vec<SyncView>>,
    changed: Mutex<usize>,
}

impl Notify for Seen {
    fn view(&self, view: &SyncView) {
        self.views.lock().unwrap().push(view.clone());
    }

    fn vault_changed(&self) {
        *self.changed.lock().unwrap() += 1;
    }
}

struct Device {
    _dir: tempfile::TempDir,
    core: SharedCore,
    sync: Syncer,
    seen: Arc<Seen>,
}

fn now() -> u64 {
    keep_vault::service::system_now_ms()
}

fn device(directory: &WindowDirectory, links: &str) -> Device {
    let dir = tempfile::Builder::new()
        .prefix("app-sync-")
        .tempdir()
        .unwrap();
    let mut core = Core::start(
        Some(VaultPaths::new(dir.path())),
        Box::new(MemoryKeystore::new()),
        Arc::new(now),
        WAIT,
    );
    if !links.is_empty() {
        let view = core.stage_text(links).unwrap();
        let fresh = view
            .rows
            .iter()
            .filter(|r| r.group == crate::core::import_view::RowGroup::Fresh)
            .count();
        core.commit_staged(
            &view.token,
            &Choice {
                fresh: vec![true; fresh],
                duplicates: Vec::new(),
            },
        )
        .unwrap();
    }
    let core = Arc::new(Mutex::new(core));
    let seen = Arc::new(Seen::default());
    let sync = Syncer::new(
        Arc::clone(&core),
        Network::Loopback(directory.clone()),
        seen.clone(),
        Arc::new(now),
    );
    Device {
        _dir: dir,
        core,
        sync,
        seen,
    }
}

fn titles(device: &Device) -> Vec<String> {
    let mut all: Vec<String> = device
        .core
        .lock()
        .unwrap()
        .list("")
        .unwrap()
        .into_iter()
        .map(|e| e.title)
        .collect();
    all.sort();
    all
}

/// Answers every question either device shows with `yes` until both sessions end.
async fn answer_until_done(devices: &[&Device], yes: bool) {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let mut running = false;
        for device in devices {
            let view = device.sync.view();
            if let Some(session) = view.session {
                running = true;
                if let Some(ask) = session.ask {
                    let _ = device.sync.answer(ask.id, yes);
                }
            }
        }
        if !running {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the sessions did not end"
        );
        tokio::time::sleep(POLL).await;
    }
}

/// A pairs B into its group: A opens a pairing window, B finds it and enters the code.
async fn pair(a: &Device, b: &Device) {
    a.sync.open_pair_window(Some("设备 A"), None).await.unwrap();
    let code = a.sync.view().session.unwrap().code.unwrap();
    let found = b.sync.search(Looking::Pair).await.unwrap();
    assert_eq!(found.len(), 1);
    b.sync
        .join_pair(&PairWith::Found { index: 0, code }, Some("设备 B"), false)
        .unwrap();
    answer_until_done(&[a, b], true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_brings_both_devices_into_one_group_with_all_entries() {
    let directory = WindowDirectory::default();
    let a = device(
        &directory,
        "otpkeep://x\notpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP",
    );
    let b = device(&directory, "otpauth://totp/B:b?secret=GEZDGNBVGY3TQOJQ");

    pair(&a, &b).await;

    for d in [&a, &b] {
        let last = d.sync.view().last.unwrap();
        assert!(last.ok, "{:?}", last.code);
        assert_eq!(last.code, "PAIRED");
        assert_eq!(titles(d), ["A", "B"]);
        let group = d.core.lock().unwrap().group_view().unwrap();
        assert!(group.in_group);
        assert_eq!(group.devices.len(), 2);
        assert!(*d.seen.changed.lock().unwrap() > 0);
    }
    let names: Vec<String> = a
        .core
        .lock()
        .unwrap()
        .group_view()
        .unwrap()
        .devices
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names.contains(&"设备 A".to_owned()) && names.contains(&"设备 B".to_owned()));
    // Both people saw the check code.
    for d in [&a, &b] {
        let views = d.seen.views.lock().unwrap();
        assert!(views.iter().filter_map(|v| v.session.as_ref()).any(|s| {
            s.ask.as_ref().is_some_and(|q| {
                q.kind == AskKind::CheckCode && q.sas.as_ref().is_some_and(|c| c.len() == 6)
            })
        }));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sync_brings_new_entries_across_after_allowing() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");
    let b = device(&directory, "");
    pair(&a, &b).await;
    let view = a
        .core
        .lock()
        .unwrap()
        .stage_text("otpauth://totp/Later:a?secret=GEZDGNBVGY3TQOJQ")
        .unwrap();
    a.core
        .lock()
        .unwrap()
        .commit_staged(
            &view.token,
            &Choice {
                fresh: vec![true],
                duplicates: Vec::new(),
            },
        )
        .unwrap();

    a.sync.open_sync_window(None).await.unwrap();
    let code = a.sync.view().session.unwrap().code.unwrap();
    assert_eq!(code.len(), 6);
    let found = b.sync.search(Looking::Sync).await.unwrap();
    b.sync.join_sync(found[0].index, &code).unwrap();
    answer_until_done(&[&a, &b], true).await;

    assert_eq!(b.sync.view().last.unwrap().code, "SYNCED");
    assert_eq!(titles(&b), ["A", "Later"]);
    let asked = a
        .seen
        .views
        .lock()
        .unwrap()
        .iter()
        .filter_map(|v| v.session.clone())
        .any(|s| {
            s.ask.is_some_and(|q| q.kind == AskKind::Allow)
                && s.peer.is_some_and(|p| p.name == "设备 B")
        });
    assert!(asked, "the person on A was asked to allow 设备 B");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_declined_request_changes_nothing() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");
    let b = device(&directory, "");
    pair(&a, &b).await;
    a.sync.open_sync_window(None).await.unwrap();
    let code = a.sync.view().session.unwrap().code.unwrap();
    let found = b.sync.search(Looking::Sync).await.unwrap();

    b.sync.join_sync(found[0].index, &code).unwrap();
    answer_until_done(&[&a, &b], false).await;

    assert_eq!(b.sync.view().last.unwrap().code, "REJECTED");
    assert!(!a.sync.view().last.unwrap().ok);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_code_is_reported_and_the_window_stays_open() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");
    let b = device(&directory, "");
    pair(&a, &b).await;
    a.sync.open_sync_window(None).await.unwrap();
    let found = b.sync.search(Looking::Sync).await.unwrap();

    b.sync.join_sync(found[0].index, "000000").unwrap();
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while b.sync.view().session.is_some() {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(POLL).await;
    }

    assert_eq!(b.sync.view().last.unwrap().code, "CODE_WRONG");
    assert!(
        a.sync.view().session.is_some(),
        "a wrong code does not close the window"
    );
    a.sync.close();
    assert_eq!(a.sync.view().last.unwrap().code, "CANCELLED");
}

#[tokio::test(flavor = "multi_thread")]
async fn one_session_at_a_time_and_sync_needs_a_group() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");

    assert_eq!(
        a.sync.open_sync_window(None).await.unwrap_err().0,
        "NOT_IN_GROUP"
    );
    a.sync.open_pair_window(None, None).await.unwrap();
    assert_eq!(
        a.sync.open_pair_window(None, None).await.unwrap_err().0,
        "SESSION_RUNNING"
    );
    assert_eq!(
        a.sync.search(Looking::Sync).await.unwrap_err().0,
        "SESSION_RUNNING"
    );
    assert!(
        a.sync.view().session.unwrap().has_invite,
        "an invite on the local network"
    );
    assert!(a.sync.invite().unwrap().contains("<svg"));
    a.sync.close();
    assert!(a.sync.invite().is_none(), "the invite goes with the window");
}

#[tokio::test(flavor = "multi_thread")]
async fn devices_are_renamed_removed_and_left() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");
    let b = device(&directory, "");
    pair(&a, &b).await;
    let mut core = a.core.lock().unwrap();

    core.rename_device("办公室").unwrap();
    assert_eq!(core.group_view().unwrap().me.unwrap().name, "办公室");
    let other = core
        .group_view()
        .unwrap()
        .devices
        .into_iter()
        .find(|d| !d.this_device)
        .unwrap();
    core.remove_device(&other.fingerprint).unwrap();
    assert_eq!(core.group_view().unwrap().devices.len(), 1);
    core.leave_group().unwrap();
    let after = core.group_view().unwrap();

    assert!(!after.in_group);
    assert_eq!(titles_of(&core), ["A"]);
}

fn titles_of(core: &Core) -> Vec<String> {
    core.list("")
        .unwrap()
        .into_iter()
        .map(|e| e.title)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_made_during_a_session_survives_its_result() {
    let directory = WindowDirectory::default();
    let a = device(&directory, "otpauth://totp/A:a?secret=JBSWY3DPEHPK3PXP");
    let before = a.core.lock().unwrap().session_start().unwrap().doc;
    let mut core = a.core.lock().unwrap();
    let edit = core
        .stage_text("otpauth://totp/Meanwhile:a?secret=GEZDGNBVGY3TQOJQ")
        .unwrap();
    core.commit_staged(
        &edit.token,
        &Choice {
            fresh: vec![true],
            duplicates: Vec::new(),
        },
    )
    .unwrap();

    // A session's result computed from the data as it was before the edit.
    let me = keep_vault::DeviceId([1; 32]);
    core.commit_session(before, &[], &[me, me]).unwrap();

    assert_eq!(titles_of(&core), ["A", "Meanwhile"]);
}

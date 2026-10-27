//! Timings that decide how the app feels with a large vault. Measured on release builds only
//! (`cargo test --release`); debug builds are not representative.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "a test harness that reports its timings"
)]

use std::time::{Duration, Instant};

use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::edit::move_to_trash_mut;
use keep_vault::master::{KDF_MEMORY_KIB, KDF_PARALLELISM, KDF_TIME_COST, derive_data_key};
use keep_vault::sealed::KdfParams;
use keep_vault::{DeviceTag, Entry, EntryId, Extra, Field, Hlc, KeyMaterial, VaultDoc};

const ENTRIES: usize = 1000;
/// Changing every entry of a large vault at once stays well below a frame of a slow animation.
const BATCH_BUDGET: Duration = Duration::from_millis(50);
/// Opening with a master password: the key derivation keeps its full strength but must not feel stuck.
const UNLOCK_BUDGET: Duration = Duration::from_secs(3);
const RUNS: usize = 5;

fn stamp() -> Hlc {
    Hlc {
        wall_ms: 1,
        logical: 0,
        device: DeviceTag([0; 8]),
    }
}

fn field<T>(value: T) -> Field<T> {
    Field {
        value,
        stamp: stamp(),
    }
}

fn demo_entry(i: usize) -> Entry {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&(i as u64).to_be_bytes());
    let mut secret = vec![7u8; 20];
    secret[..8].copy_from_slice(&(i as u64).to_be_bytes());
    Entry {
        id: EntryId(id),
        created_at: stamp(),
        title: field(format!("Example Cloud {i}")),
        issuer: field("Example Cloud".to_owned()),
        account: field(format!("alex{i}@example.com")),
        key: field(KeyMaterial {
            secret: SecretKey::from_bytes(secret).unwrap(),
            spec: CodeSpec::default(),
            counter: 0,
            key_id: stamp(),
        }),
        note: field("demo note ".repeat(20)),
        tags: field(vec!["work".to_owned()]),
        rank: field(format!("{i:04}")),
        pinned: field(false),
        keep_duplicate: field(false),
        trashed_at: field(None),
        old_key_of: field(None),
        extra: Extra::new(),
    }
}

fn demo_doc() -> VaultDoc {
    VaultDoc {
        entries: (0..ENTRIES).map(demo_entry).collect(),
        ..VaultDoc::blank()
    }
}

/// The median of `RUNS` timings of `f`.
fn median(mut f: impl FnMut()) -> Duration {
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    times.sort();
    times[RUNS / 2]
}

#[cfg_attr(
    debug_assertions,
    ignore = "timing is only meaningful in release builds"
)]
#[test]
fn trashing_all_of_1000_entries_in_one_change_stays_fast() {
    let doc = demo_doc();
    let ids: Vec<EntryId> = doc.entries.iter().map(|e| e.id).collect();

    let took = median(|| {
        // One copy of the vault for the whole batch, as the main window's batch actions make it.
        let mut next = doc.clone();
        for id in &ids {
            move_to_trash_mut(&mut next, *id, 2).unwrap();
        }
        assert!(next.entries.iter().all(|e| e.trashed_at.value.is_some()));
    });

    println!("batch trash of {ENTRIES}: {took:?}");
    assert!(took <= BATCH_BUDGET, "took {took:?}");
}

#[cfg_attr(
    debug_assertions,
    ignore = "timing is only meaningful in release builds"
)]
#[test]
fn deriving_the_key_at_full_strength_takes_seconds_at_most() {
    let params = KdfParams {
        salt: [6; 16],
        m_cost_kib: KDF_MEMORY_KIB,
        t_cost: KDF_TIME_COST,
        p_cost: KDF_PARALLELISM,
    };

    let took = median(|| {
        let _ = derive_data_key("demo password", &params).unwrap();
    });

    println!("Argon2id 64 MiB, t=3, p=1: {took:?}");
    assert!(took <= UNLOCK_BUDGET, "took {took:?}");
}

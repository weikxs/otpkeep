//! Deterministic sync simulation (design/09-testing.md §2.6).
//!
//! Random groups of 3–16 devices edit, trash, delete, replace keys, step counters, add duplicates, rename and
//! remove devices, with skewed and stepped-back clocks, and sync in random pairs with random disconnects. After
//! the activity stops and every pair of members has synced enough, five properties must hold:
//!
//! 1. all members hold the same state (equal digests);
//! 2. no key vanished silently: every key ever seen is still in an entry (trash and old-key copies included),
//!    or an entry that held it was deleted by a user (permanent deletion or trash expiry);
//! 3. a key not marked "keep duplicate" is in at most one entry outside the trash;
//! 4. a tombstone is cleared only when every member holds it and it is past the grace period, and its entry
//!    never comes back afterwards;
//! 5. a removed device cannot complete a sync with anyone who knows of the removal.
//!
//! `OTPKEEP_SIM_SEEDS` sets the number of seeds (`make sim` runs 10 000 on a release build, `make check` 300);
//! `OTPKEEP_SIM_SEED` replays one. A plain debug `cargo test` runs a few.

#[cfg(test)]
mod sim;

use sim::{Rng, World};

const DEFAULT_SEEDS: u64 = 8;
const STEPS: usize = 300;

fn seeds() -> Vec<u64> {
    if let Some(one) = std::env::var("OTPKEEP_SIM_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        return vec![one];
    }
    let count = std::env::var("OTPKEEP_SIM_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SEEDS);
    (0..count).collect()
}

#[test]
fn every_seed_converges_without_losing_keys() {
    for seed in seeds() {
        let mut rng = Rng::new(seed);
        let mut world = World::new(&mut rng);
        for _ in 0..STEPS {
            world.step(&mut rng, seed);
        }
        world.settle(seed);
        world.check_final(seed);
    }
}

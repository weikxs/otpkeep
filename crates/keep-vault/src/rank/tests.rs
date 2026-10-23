use proptest::prelude::*;

use super::{RankError, after, before, between, first};

#[test]
fn the_first_key_sits_in_the_middle_of_the_range() {
    let key = first();

    assert!(before(&key) < key);
    assert!(after(&key) > key);
}

#[test]
fn appending_always_sorts_later() {
    let mut keys = vec![first()];
    for _ in 0..500 {
        let next = after(keys.last().unwrap());
        assert!(next > *keys.last().unwrap());
        keys.push(next);
    }
    assert!(
        // Keys grow by one character about every 31 appends: 500 entries stay under 20 characters.
        keys.iter().all(|k| k.len() <= 20),
        "appending must not grow keys quickly"
    );
}

#[test]
fn prepending_always_sorts_earlier() {
    let mut key = first();
    for _ in 0..500 {
        let earlier = before(&key);
        assert!(earlier < key, "{earlier} < {key}");
        assert!(!earlier.is_empty());
        key = earlier;
    }
}

#[test]
fn inserting_between_neighbours_lands_between_them() {
    let (a, b) = (first(), after(&first()));
    let middle = between(&a, &b).unwrap();

    assert!(a < middle && middle < b, "{a} < {middle} < {b}");
}

#[test]
fn repeated_inserts_at_the_same_place_keep_working() {
    let low = first();
    let mut high = after(&low);
    for _ in 0..200 {
        let middle = between(&low, &high).unwrap();
        assert!(low < middle && middle < high);
        high = middle;
    }
}

#[test]
fn equal_or_reversed_neighbours_are_an_error() {
    let key = first();

    assert_eq!(between(&key, &key), Err(RankError::NotOrdered));
    assert_eq!(between(&after(&key), &key), Err(RankError::NotOrdered));
}

#[test]
fn keys_use_only_digits_and_letters() {
    let key = between(&first(), &after(&first())).unwrap();

    assert!(key.chars().all(|c| c.is_ascii_alphanumeric()), "{key}");
}

proptest! {
    #[test]
    fn between_any_two_ordered_keys(steps in proptest::collection::vec(0u8..3, 1..60)) {
        // Build an arbitrary sequence of keys with random operations, then check every gap.
        let mut keys = vec![first()];
        for step in steps {
            let last = keys.len() - 1;
            let key = match step {
                0 => after(&keys[last]),
                1 => before(&keys[0]),
                _ if keys.len() > 1 => between(&keys[0], &keys[1]).unwrap(),
                _ => after(&keys[last]),
            };
            keys.push(key);
            keys.sort();
            keys.dedup();
        }
        for pair in keys.windows(2) {
            let middle = between(&pair[0], &pair[1]).unwrap();
            prop_assert!(pair[0] < middle && middle < pair[1], "{} < {} < {}", pair[0], middle, pair[1]);
        }
    }
}

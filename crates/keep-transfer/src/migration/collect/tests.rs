use keep_passcode::{CodeSpec, SecretKey};

use super::{Added, MigrationCollector};
use crate::migration::{MigrationBatch, MigrationItem};

fn batch(id: i32, size: u32, index: u32, secret: u8) -> MigrationBatch {
    MigrationBatch {
        version: 1,
        batch_id: id,
        batch_size: size,
        batch_index: index,
        items: vec![MigrationItem {
            secret: SecretKey::from_bytes(vec![secret; 20]).unwrap(),
            issuer: "Example Cloud".to_owned(),
            account: format!("account-{secret}"),
            spec: CodeSpec::default(),
            counter: 0,
            assumed: vec![],
        }],
        unreadable: vec![],
        encoded_len: 0,
    }
}

#[test]
fn reports_progress_and_completes_in_any_order() {
    let (c, first) = MigrationCollector::default().with(batch(7, 3, 2, 30));
    let (c, second) = c.with(batch(7, 3, 0, 10));
    let status = &c.groups()[0];

    assert_eq!(first, Added::New);
    assert_eq!(second, Added::New);
    assert_eq!(
        (status.received.clone(), status.expected, status.complete),
        (vec![0, 2], 3, false)
    );
    let (c, _) = c.with(batch(7, 3, 1, 20));
    assert!(c.groups()[0].complete);
    let accounts: Vec<_> = c
        .take(7)
        .unwrap()
        .iter()
        .map(|i| i.account.clone())
        .collect();
    assert_eq!(
        accounts,
        ["account-10", "account-20", "account-30"],
        "items come in code order"
    );
}

#[test]
fn scanning_the_same_code_twice_counts_once() {
    let (c, _) = MigrationCollector::default().with(batch(7, 2, 0, 1));
    let (c, again) = c.with(batch(7, 2, 0, 1));

    assert_eq!(again, Added::AlreadyHad);
    assert_eq!(c.groups()[0].received, [0]);
}

#[test]
fn different_exports_are_kept_apart() {
    let (c, _) = MigrationCollector::default().with(batch(7, 2, 0, 1));
    let (c, _) = c.with(batch(8, 1, 0, 2));

    let ids: Vec<i32> = c.groups().iter().map(|g| g.batch_id).collect();
    assert_eq!(ids, [7, 8]);
    assert!(c.groups()[1].complete);
}

#[test]
fn a_disagreeing_batch_size_keeps_the_first_and_warns() {
    let (c, _) = MigrationCollector::default().with(batch(7, 3, 0, 1));
    let (c, _) = c.with(batch(7, 2, 1, 2));

    let status = &c.groups()[0];
    assert_eq!(status.expected, 3);
    assert!(status.size_conflict);
}

#[test]
fn a_code_outside_its_batch_is_refused() {
    let (c, added) = MigrationCollector::default().with(batch(7, 2, 5, 1));

    assert_eq!(added, Added::OutOfRange);
    assert!(c.groups().is_empty());
}

#[test]
fn a_missing_size_counts_as_one_code() {
    let (c, _) = MigrationCollector::default().with(batch(7, 0, 0, 1));

    assert_eq!((c.groups()[0].expected, c.groups()[0].complete), (1, true));
}

#[test]
fn what_was_received_can_be_taken_before_the_rest_arrives() {
    let (c, _) = MigrationCollector::default().with(batch(7, 3, 1, 20));

    assert_eq!(c.take(7).unwrap().len(), 1);
    assert!(c.take(9).is_none());
}

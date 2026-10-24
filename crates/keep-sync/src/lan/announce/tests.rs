use super::{AnnounceError, Announcement, HOUR_MS, from_txt, group_tag, matches_group, to_txt};
use crate::lan::window::WindowKind;

const TAG_KEY: [u8; 32] = [3; 32];
const GROUP: [u8; 16] = [4; 16];
const NOW: u64 = 1_700_000_000_000;

fn sample(kind: WindowKind) -> Announcement {
    Announcement {
        kind,
        lan_name: "书房电脑".to_owned(),
        short_fingerprint: "7F3A".to_owned(),
        remaining_s: 95,
        window_random: [0xab; 16],
        group_tag: (kind == WindowKind::Sync).then(|| group_tag(&TAG_KEY, &GROUP, NOW / HOUR_MS)),
    }
}

#[test]
fn an_announcement_survives_the_txt_record() {
    for kind in [WindowKind::Pair, WindowKind::Sync] {
        let announcement = sample(kind);

        assert_eq!(from_txt(&to_txt(&announcement)), Ok(announcement));
    }
}

#[test]
fn the_txt_record_holds_no_ids_or_codes() {
    let txt = to_txt(&sample(WindowKind::Sync));
    let keys: Vec<&str> = txt.iter().map(|(k, _)| k.as_str()).collect();

    assert_eq!(keys, ["v", "w", "n", "f", "x", "r", "g"]);
    assert!(
        to_txt(&sample(WindowKind::Pair))
            .iter()
            .all(|(k, _)| k != "g")
    );
}

#[test]
fn records_from_other_versions_or_with_bad_values_are_ignored() {
    let good = to_txt(&sample(WindowKind::Sync));
    let with = |key: &str, value: &str| {
        good.iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    if k == key {
                        value.to_owned()
                    } else {
                        v.clone()
                    },
                )
            })
            .collect::<Vec<_>>()
    };
    let without = |key: &str| {
        good.iter()
            .filter(|(k, _)| k != key)
            .cloned()
            .collect::<Vec<_>>()
    };

    assert_eq!(from_txt(&with("v", "2")), Err(AnnounceError::Version));
    assert_eq!(from_txt(&with("w", "chat")), Err(AnnounceError::Invalid));
    assert_eq!(from_txt(&with("n", "")), Err(AnnounceError::Invalid));
    assert_eq!(from_txt(&with("f", "XYZ")), Err(AnnounceError::Invalid));
    assert_eq!(from_txt(&with("x", "soon")), Err(AnnounceError::Invalid));
    assert_eq!(from_txt(&with("r", "AAAA")), Err(AnnounceError::Invalid));
    assert_eq!(
        from_txt(&without("g")),
        Err(AnnounceError::Invalid),
        "sync windows carry a group tag"
    );
    assert_eq!(from_txt(&without("r")), Err(AnnounceError::Invalid));
}

#[test]
fn a_group_tag_matches_for_the_hour_before_and_after_only() {
    let tag = group_tag(&TAG_KEY, &GROUP, NOW / HOUR_MS);

    assert!(matches_group(&tag, &TAG_KEY, &GROUP, NOW));
    assert!(matches_group(&tag, &TAG_KEY, &GROUP, NOW + HOUR_MS));
    assert!(matches_group(&tag, &TAG_KEY, &GROUP, NOW - HOUR_MS));
    assert!(!matches_group(&tag, &TAG_KEY, &GROUP, NOW + 2 * HOUR_MS));
    assert!(
        !matches_group(&tag, &[5; 32], &GROUP, NOW),
        "another group's key"
    );
    assert!(!matches_group(&tag, &TAG_KEY, &[6; 16], NOW));
}

#[test]
fn group_tags_change_every_hour() {
    let hour = NOW / HOUR_MS;

    assert_ne!(
        group_tag(&TAG_KEY, &GROUP, hour),
        group_tag(&TAG_KEY, &GROUP, hour + 1)
    );
}

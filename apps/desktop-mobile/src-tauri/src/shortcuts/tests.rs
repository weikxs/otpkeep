use std::collections::BTreeMap;

use super::{Holder, ShortcutError, ShortcutMap, Target, canonical};

fn mac() -> ShortcutMap {
    ShortcutMap::new(true, BTreeMap::new())
}

#[test]
fn canonical_form_is_stable_whatever_the_spelling() {
    assert_eq!(
        canonical("Control+Alt+Super+KeyG"),
        Ok("control+alt+super+KeyG".to_owned())
    );
    assert_eq!(
        canonical("ctrl+option+cmd+g"),
        Ok("control+alt+super+KeyG".to_owned())
    );
    assert_eq!(canonical(" Shift+Alt+F5 "), Ok("shift+alt+F5".to_owned()));
}

#[test]
fn refuses_keys_without_a_real_modifier() {
    assert_eq!(canonical("KeyG"), Err(ShortcutError::Invalid));
    assert_eq!(canonical("Shift+KeyG"), Err(ShortcutError::Invalid));
    assert_eq!(canonical("Control+Alt"), Err(ShortcutError::Invalid));
    assert_eq!(canonical("Control+NoSuchKey"), Err(ShortcutError::Invalid));
    assert_eq!(canonical(""), Err(ShortcutError::Invalid));
}

#[test]
fn macos_gives_the_first_ten_rows_defaults() {
    let map = mac();

    assert_eq!(
        map.default_for(0).as_deref(),
        Some("control+alt+super+Digit1")
    );
    assert_eq!(
        map.default_for(9).as_deref(),
        Some("control+alt+super+Digit0")
    );
    assert_eq!(map.default_for(10), None);
    assert_eq!(map.targets().len(), 10);
}

#[test]
fn other_systems_have_no_defaults() {
    let map = ShortcutMap::new(false, BTreeMap::new());

    assert_eq!(map.default_for(0), None);
    assert!(map.targets().is_empty());
}

#[test]
fn an_entry_takes_a_free_shortcut_and_shows_it_over_its_row_default() {
    let map = mac().with("e1", Some("control+alt+super+KeyG")).unwrap();

    assert_eq!(map.own("e1"), Some("control+alt+super+KeyG"));
    assert_eq!(
        map.shown_for("e1", 0).as_deref(),
        Some("control+alt+super+KeyG")
    );
    assert_eq!(
        map.shown_for("e2", 1).as_deref(),
        Some("control+alt+super+Digit2")
    );
    assert!(map.targets().contains(&(
        "control+alt+super+KeyG".to_owned(),
        Target::Entry("e1".to_owned())
    )));
}

#[test]
fn names_who_already_uses_a_shortcut() {
    let map = mac().with("e1", Some("control+alt+super+KeyG")).unwrap();

    assert_eq!(
        map.with("e2", Some("control+alt+super+KeyG")),
        Err(ShortcutError::Taken(Holder::Entry("e1".to_owned())))
    );
    assert_eq!(
        map.with("e2", Some("control+alt+super+Digit3")),
        Err(ShortcutError::Taken(Holder::Row(2)))
    );
    assert_eq!(
        map.with("e2", Some("control+alt+super+Space")),
        Err(ShortcutError::Taken(Holder::SearchPanel))
    );
}

#[test]
fn setting_the_same_shortcut_again_or_removing_it_works() {
    let map = mac().with("e1", Some("control+alt+super+KeyG")).unwrap();

    assert!(map.with("e1", Some("control+alt+super+KeyG")).is_ok());
    let cleared = map.with("e1", None).unwrap();
    assert_eq!(cleared.own("e1"), None);
    assert_eq!(
        map.own("e1"),
        Some("control+alt+super+KeyG"),
        "the original is unchanged"
    );
}

use keep_passcode::{CodeSpec, SecretKey};
use keep_vault::edit::{NOTE_MAX_CHARS, NewEntry, TITLE_MAX_CHARS, add_entry, move_to_trash};
use keep_vault::{DeviceTag, Extra, Hlc, VaultDoc};

use super::{Candidate, Choice, ImportError, Lost, Origin, commit_import, preview};

fn empty() -> VaultDoc {
    VaultDoc {
        vault_id: [0; 16],
        entries: vec![],
        tombstones: vec![],
        group: None,
        device: None,
        clock: Hlc {
            wall_ms: 0,
            logical: 0,
            device: DeviceTag([0; 8]),
        },
        device_tag: DeviceTag([1; 8]),
        extra: Extra::new(),
    }
}

fn candidate(title: &str, secret: u8) -> Candidate {
    Candidate {
        title: title.to_owned(),
        issuer: "Example Cloud".to_owned(),
        account: "alex@example.com".to_owned(),
        secret: SecretKey::from_bytes(vec![secret; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: vec![],
        origin: Origin::Item(0),
    }
}

fn with_entry(title: &str, secret: u8) -> VaultDoc {
    let new = NewEntry {
        title: title.to_owned(),
        issuer: String::new(),
        account: String::new(),
        secret: SecretKey::from_bytes(vec![secret; 20]).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        note: String::new(),
        tags: vec![],
        keep_duplicate: false,
    };
    add_entry(&empty(), new, 1).unwrap().0
}

#[test]
fn sorts_new_accounts_from_ones_already_in_the_vault() {
    let doc = with_entry("Existing", 1);
    let existing = doc.entries[0].id;

    let p = preview(vec![candidate("Same secret", 1), candidate("New", 2)], &doc);

    assert_eq!(p.fresh.len(), 1);
    assert_eq!(p.fresh[0].candidate.title, "New");
    assert_eq!(p.duplicates.len(), 1);
    assert_eq!(p.duplicates[0].existing, Some(existing));
}

#[test]
fn a_secret_in_the_trash_is_a_duplicate_too() {
    let doc = with_entry("Existing", 1);
    let doc = move_to_trash(&doc, doc.entries[0].id, 2).unwrap();

    assert_eq!(preview(vec![candidate("x", 1)], &doc).duplicates.len(), 1);
}

#[test]
fn a_secret_repeated_inside_the_import_is_a_duplicate_of_the_first() {
    let p = preview(
        vec![candidate("first", 3), candidate("second", 3)],
        &empty(),
    );

    assert_eq!(p.fresh.len(), 1);
    assert_eq!(p.duplicates[0].existing, None);
    assert_eq!(p.duplicates[0].same_as_fresh, Some(0));
}

#[test]
fn flags_a_name_that_is_already_taken() {
    let p = preview(vec![candidate("existing", 2)], &with_entry("Existing", 1));

    assert!(p.fresh[0].title_taken);
}

#[test]
fn fits_overlong_values_and_says_what_was_cut() {
    let mut long = candidate(&"名".repeat(TITLE_MAX_CHARS + 5), 1);
    long.note = "n".repeat(NOTE_MAX_CHARS + 1);
    long.tags = (0..25).map(|i| format!("tag{i}")).collect();

    let p = preview(vec![long], &empty());

    let fitted = &p.fresh[0];
    assert_eq!(fitted.candidate.title.chars().count(), TITLE_MAX_CHARS);
    assert_eq!(fitted.candidate.note.chars().count(), NOTE_MAX_CHARS);
    assert_eq!(fitted.candidate.tags.len(), 20);
    assert_eq!(
        fitted.lost,
        [Lost::TitleShortened, Lost::NoteShortened, Lost::TagsDropped]
    );
}

#[test]
fn an_untitled_account_is_named_after_its_service_or_account() {
    let mut untitled = candidate("  ", 1);
    let mut nameless = candidate("", 2);
    nameless.issuer = String::new();

    untitled.issuer = "Demo Git".into();
    let p = preview(vec![untitled, nameless], &empty());

    assert_eq!(p.fresh[0].candidate.title, "Demo Git");
    assert_eq!(p.fresh[1].candidate.title, "alex@example.com");
}

#[test]
fn an_issuer_account_title_keeps_only_the_issuer() {
    let mut no_issuer = candidate("Example Cloud:Alex@Example.com", 1);
    no_issuer.issuer = String::new();
    let with_issuer = candidate("Demo Git: alex@example.com", 2);
    let other_account = candidate("Example Mail:ops@example.com", 3);
    let no_account = {
        let mut c = candidate("Example Bank:alex@example.com", 4);
        c.account = String::new();
        c
    };

    let p = preview(
        vec![no_issuer, with_issuer, other_account, no_account],
        &empty(),
    );

    assert_eq!(p.fresh[0].candidate.title, "Example Cloud");
    assert_eq!(p.fresh[0].candidate.issuer, "Example Cloud");
    assert_eq!(p.fresh[0].candidate.account, "alex@example.com");
    assert_eq!(p.fresh[1].candidate.title, "Demo Git");
    assert_eq!(
        p.fresh[1].candidate.issuer, "Example Cloud",
        "a known issuer is kept"
    );
    assert_eq!(p.fresh[2].candidate.title, "Example Mail:ops@example.com");
    assert_eq!(p.fresh[3].candidate.title, "Example Bank:alex@example.com");
}

#[test]
fn commits_only_what_was_chosen_and_only_adds() {
    let doc = with_entry("Existing", 1);
    let p = preview(
        vec![candidate("dup", 1), candidate("a", 2), candidate("b", 3)],
        &doc,
    );

    let (doc, added) = commit_import(
        &doc,
        &p,
        &Choice {
            fresh: vec![true, false],
            duplicates: vec![true],
        },
        10,
    )
    .unwrap();

    assert_eq!(added.len(), 2);
    assert_eq!(doc.entries.len(), 3);
    let dup = doc.entries.iter().find(|e| e.title.value == "dup").unwrap();
    assert!(
        dup.keep_duplicate.value,
        "a duplicate imported anyway is marked"
    );
    assert_eq!(
        doc.entries[0].title.value, "Existing",
        "existing entries are untouched"
    );
}

#[test]
fn a_choice_that_does_not_fit_the_preview_is_refused() {
    let p = preview(vec![candidate("a", 2)], &empty());

    assert_eq!(
        commit_import(
            &empty(),
            &p,
            &Choice {
                fresh: vec![true, true],
                duplicates: vec![]
            },
            1
        )
        .err(),
        Some(ImportError::ChoiceMismatch)
    );
}

#[test]
fn a_vault_that_changed_since_the_preview_is_refused() {
    let p = preview(vec![candidate("a", 2)], &empty());
    let changed = with_entry("Added meanwhile", 2);

    assert_eq!(
        commit_import(&changed, &p, &Choice::all(&p), 1).err(),
        Some(ImportError::VaultChanged)
    );
}

#[test]
fn sources_become_candidates_with_their_origins() {
    let link = "otpauth://totp/Demo:alex?secret=JBSWY3DPEHPK3PXP&issuer=Demo";
    let sheet = keep_keyuri::read_sheet(&format!("# My demo\n# note: a note\n{link}\n"));
    let parsed = keep_keyuri::parse_key_uri(link).unwrap();

    let from_sheet = super::from_sheet(&sheet);
    let from_link = super::from_link(&parsed);

    assert_eq!(from_sheet[0].title, "My demo");
    assert_eq!(from_sheet[0].note, "a note");
    assert_eq!(from_sheet[0].origin, Origin::Line(3));
    assert_eq!(
        (from_link.issuer.as_str(), from_link.account.as_str()),
        ("Demo", "alex")
    );
}

#[test]
fn transfer_codes_become_candidates_with_code_and_position() {
    use crate::migration::{
        BATCH_BUDGET_BYTES, ExportItem, decode_migration_link, encode_migration,
    };
    let secrets: Vec<SecretKey> = (1..=2u8)
        .map(|b| SecretKey::from_bytes(vec![b; 20]).unwrap())
        .collect();
    let items: Vec<ExportItem<'_>> = secrets
        .iter()
        .map(|secret| ExportItem {
            secret,
            spec: CodeSpec::default(),
            counter: 0,
            issuer: "Example Cloud",
            account: "alex",
        })
        .collect();
    let export = encode_migration(&items, BATCH_BUDGET_BYTES).unwrap();
    let codes: Vec<_> = export
        .links
        .iter()
        .map(|l| decode_migration_link(l).unwrap())
        .collect();

    let candidates = super::from_migration(&codes);

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[1].origin, Origin::Code { code: 0, index: 1 });
    assert_eq!(candidates[0].issuer, "Example Cloud");
    assert_eq!(
        preview(candidates, &empty()).fresh[0].candidate.title,
        "Example Cloud"
    );
}

use keep_passcode::{CodeSpec, SecretKey};

use super::{SheetEntry, SheetProblemKind, read_sheet, write_sheet};
use crate::{KeyUri, KeyUriFault};

const DEMO: &str = "JBSWY3DPEHPK3PXP";

fn uri(issuer: &str, account: &str) -> KeyUri {
    KeyUri {
        secret: SecretKey::from_bytes(b"Hello!\xDE\xAD\xBE\xEF".to_vec()).unwrap(),
        spec: CodeSpec::default(),
        counter: 0,
        issuer: issuer.to_owned(),
        account: account.to_owned(),
    }
}

#[test]
fn writes_titles_notes_and_links_separated_by_blank_lines() {
    let first = uri("Example Cloud", "alex");
    let second = uri("", "demo-user");
    let entries = [
        SheetEntry {
            title: "Cloud (work)",
            note: "line one\nline two \\ done",
            uri: &first,
        },
        SheetEntry {
            title: "Demo",
            note: "",
            uri: &second,
        },
    ];

    let text = write_sheet(&entries);

    assert_eq!(
        *text,
        format!(
            "# Cloud (work)\n# note: line one\\nline two \\\\ done\notpauth://totp/Example%20Cloud:alex?secret={DEMO}&issuer=Example%20Cloud\n\n# Demo\notpauth://totp/demo-user?secret={DEMO}\n"
        )
    );
}

#[test]
fn reads_back_what_it_writes() {
    let first = uri("Example Cloud", "alex");
    let entries = [SheetEntry {
        title: "Cloud (work)",
        note: "line one\nline two \\ done",
        uri: &first,
    }];

    let read = read_sheet(&write_sheet(&entries));

    assert!(read.problems.is_empty(), "{:?}", read.problems);
    assert_eq!(read.items.len(), 1);
    assert_eq!(read.items[0].title.as_deref(), Some("Cloud (work)"));
    assert_eq!(
        read.items[0].note.as_deref(),
        Some("line one\nline two \\ done")
    );
    assert_eq!(read.items[0].uri.issuer, "Example Cloud");
}

#[test]
fn titles_in_a_single_line_and_without_surrounding_spaces() {
    let one = uri("", "a");
    let entries = [SheetEntry {
        title: "  two\nlines  ",
        note: "",
        uri: &one,
    }];

    assert!(write_sheet(&entries).starts_with("# two lines\n"));
}

#[test]
fn reads_a_plain_list_of_links() {
    let text = format!("otpauth://totp/a?secret={DEMO}\notpauth://totp/b?secret={DEMO}\n");

    let read = read_sheet(&text);

    assert_eq!(read.items.len(), 2);
    assert!(
        read.items
            .iter()
            .all(|item| item.title.is_none() && item.note.is_none())
    );
}

#[test]
fn a_title_belongs_only_to_the_next_link() {
    let text = format!(
        "# First\notpauth://totp/a?secret={DEMO}\notpauth://totp/b?secret={DEMO}\n# Dangling\n"
    );

    let read = read_sheet(&text);

    assert_eq!(read.items[0].title.as_deref(), Some("First"));
    assert_eq!(read.items[1].title, None);
    assert!(read.problems.is_empty());
}

#[test]
fn reports_bad_lines_with_their_numbers_and_keeps_going() {
    let text = format!(
        "# A\notpauth://totp/a?secret={DEMO}\n\nthis is not a link\notpauth://totp/b?secret=\n# C\r\notpauth://totp/c?secret={DEMO}\r\n"
    );

    let read = read_sheet(&text);

    assert_eq!(read.items.len(), 2);
    assert_eq!(read.items[1].title.as_deref(), Some("C"));
    assert_eq!((read.items[0].line, read.items[1].line), (2, 7));
    assert_eq!(read.problems.len(), 2);
    assert_eq!(read.problems[0].line, 4);
    assert_eq!(read.problems[0].kind, SheetProblemKind::NotALink);
    assert_eq!(read.problems[1].line, 5);
    assert_eq!(
        read.problems[1].kind,
        SheetProblemKind::BadLink(KeyUriFault::SecretMissing)
    );
}

#[test]
fn a_note_without_a_title_is_kept() {
    let text = format!("# note: only a note\notpauth://totp/a?secret={DEMO}\n");

    let read = read_sheet(&text);

    assert_eq!(read.items[0].title, None);
    assert_eq!(read.items[0].note.as_deref(), Some("only a note"));
}

#[test]
fn empty_text_has_nothing_in_it() {
    let read = read_sheet("");

    assert!(read.items.is_empty());
    assert!(read.problems.is_empty());
}

#[test]
fn a_bare_hash_line_is_ignored() {
    let text = format!("#\n# Title\n#\notpauth://totp/a?secret={DEMO}\n");

    let read = read_sheet(&text);

    assert_eq!(read.items[0].title.as_deref(), Some("Title"));
    assert!(read.problems.is_empty());
}

#[test]
fn unknown_escapes_in_a_note_are_kept_as_written() {
    let text = format!("# note: tab\\there and a trailing \\\notpauth://totp/a?secret={DEMO}\n");

    let read = read_sheet(&text);

    assert_eq!(
        read.items[0].note.as_deref(),
        Some("tab\\there and a trailing \\")
    );
}

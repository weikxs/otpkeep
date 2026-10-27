//! How an account is named in the menu: a short name and, when it tells the account apart, a second part.
//! Display only: what is stored is never changed.
//!
//! Imported names often repeat the account (`Example Cloud:demo@example.com`, or a title that is just the
//! account), so the name drops what the second part shows, and the second part is left out when the name
//! already says it.

use crate::core::PanelEntry;

/// Longest name, in characters.
const NAME_CHARS: usize = 24;
/// Longest note shown as the second part, in characters.
const NOTE_CHARS: usize = 20;
/// Longest account name shown as the second part, in characters.
const SECONDARY_CHARS: usize = 24;

/// `Name · Secondary`, or the name alone.
pub(super) fn label(entry: &PanelEntry) -> String {
    let name = name(entry);
    let note = collapse(&entry.note);
    let secondary = if note.is_empty() {
        let account = collapse(&entry.account);
        let shown = name.to_lowercase();
        if account.is_empty() || shown.contains(&account.to_lowercase()) {
            String::new()
        } else {
            shorten(&account, SECONDARY_CHARS)
        }
    } else {
        shorten(&note, NOTE_CHARS)
    };
    let name = shorten(&name, NAME_CHARS);
    if secondary.is_empty() {
        name
    } else {
        format!("{name} · {secondary}")
    }
}

/// The name to show: the part before `:` of a title like `Issuer:account`, the issuer when the title is just
/// the account, otherwise the title.
fn name(entry: &PanelEntry) -> String {
    let title = collapse(&entry.title);
    let account = collapse(&entry.account).to_lowercase();
    let issuer = collapse(&entry.issuer);
    if !account.is_empty() {
        if let Some((head, rest)) = title.split_once(':') {
            let head = head.trim();
            if !head.is_empty() && rest.trim().to_lowercase().contains(&account) {
                return head.to_owned();
            }
        }
        if !issuer.is_empty() && title.to_lowercase() == account {
            return issuer;
        }
    }
    title
}

/// `text` with every run of white space (new lines too) made one space, and trimmed.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` cut to `most` characters (not bytes), ending in `…` when cut.
pub(super) fn shorten(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let kept: String = text.chars().take(most.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::{label, shorten};
    use crate::core::{EntryKind, PanelEntry};

    fn entry(title: &str, issuer: &str, account: &str, note: &str) -> PanelEntry {
        PanelEntry {
            id: "e1".to_owned(),
            title: title.to_owned(),
            issuer: issuer.to_owned(),
            account: account.to_owned(),
            note: note.to_owned(),
            tags: Vec::new(),
            kind: EntryKind::Totp,
            pinned: false,
            code: None,
            next_code: None,
            period_s: None,
            valid_until_ms: None,
            shortcut: None,
            own_shortcut: None,
        }
    }

    #[test]
    fn an_issuer_account_title_shows_the_issuer_and_the_account_once() {
        let imported = entry("Example Cloud:ops@example.com", "", "ops@example.com", "");

        assert_eq!(label(&imported), "Example Cloud · ops@example.com");
    }

    #[test]
    fn the_part_after_the_colon_may_hold_more_than_the_account() {
        let imported = entry("Example Cloud:demo-user@123456789012", "", "demo-user", "");

        assert_eq!(label(&imported), "Example Cloud · demo-user");
    }

    #[test]
    fn a_title_that_is_the_account_shows_the_issuer() {
        let bare = entry("123456789012", "Example Cloud", "123456789012", "");
        let no_issuer = entry("123456789012", "", "123456789012", "");

        assert_eq!(label(&bare), "Example Cloud · 123456789012");
        assert_eq!(label(&no_issuer), "123456789012");
    }

    #[test]
    fn a_note_is_the_second_part_when_there_is_one() {
        let noted = entry(
            "Example Mail",
            "Example Mail",
            "demo@example.com",
            "Backup  codes\nin the drawer by the door",
        );

        assert_eq!(label(&noted), "Example Mail · Backup codes in the…");
    }

    #[test]
    fn long_names_and_accounts_are_shortened() {
        let long = entry(
            "Example Cloud Platform Console",
            "",
            "a.very.long.demo.address@example.com",
            "",
        );

        assert_eq!(
            label(&long),
            "Example Cloud Platform… · a.very.long.demo.addres…"
        );
    }

    #[test]
    fn chinese_is_shortened_by_characters() {
        let chinese = entry(
            "示例银行",
            "示例银行",
            "",
            "备用码放在书房抽屉里的蓝色信封中，千万别弄丢",
        );

        assert_eq!(
            label(&chinese),
            "示例银行 · 备用码放在书房抽屉里的蓝色信封中，千万…"
        );
        assert_eq!(shorten("一二三四五六", 4), "一二三…");
    }

    #[test]
    fn a_title_with_a_colon_but_another_account_is_kept() {
        let other = entry("Demo Git:work", "", "demo@example.com", "");

        assert_eq!(label(&other), "Demo Git:work · demo@example.com");
    }
}

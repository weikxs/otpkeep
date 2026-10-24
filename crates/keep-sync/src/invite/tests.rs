use super::{InviteError, PairInvite, new_code};

fn invite() -> PairInvite {
    PairInvite {
        addrs: vec!["192.168.1.20".parse().unwrap(), "fd12::5".parse().unwrap()],
        port: 51_234,
        short_fingerprint: "7F3A".to_owned(),
        window_random: [0x11; 16],
        secret: [0x22; 16],
    }
}

#[test]
fn an_invite_survives_the_qr_code() {
    let text = invite().encode();

    assert!(text.starts_with("otpkeep-pair:1?"), "{}", &*text);
    assert_eq!(PairInvite::parse(&text), Ok(invite()));
}

#[test]
fn invites_with_public_addresses_or_bad_values_are_refused() {
    let good = invite().encode();
    let swap = |from: &str, to: &str| PairInvite::parse(&good.replacen(from, to, 1));

    assert_eq!(swap("192.168.1.20", "8.8.8.8"), Err(InviteError::NotLocal));
    assert_eq!(swap("p=51234", "p=0"), Err(InviteError::Invalid));
    assert_eq!(swap("f=7F3A", "f=7F3"), Err(InviteError::Invalid));
    assert_eq!(
        swap("otpkeep-pair:1", "otpkeep-pair:2"),
        Err(InviteError::Version)
    );
    assert_eq!(
        PairInvite::parse("otpauth://totp/x?secret=AAAA"),
        Err(InviteError::NotAnInvite)
    );
    let no_secret: String = good
        .split('&')
        .filter(|p| !p.starts_with("s="))
        .collect::<Vec<_>>()
        .join("&");
    assert_eq!(PairInvite::parse(&no_secret), Err(InviteError::Invalid));
}

#[test]
fn codes_have_the_right_number_of_digits_and_vary() {
    let sync: Vec<String> = (0..50).map(|_| new_code(6).unwrap().to_string()).collect();
    let pair = new_code(8).unwrap();

    assert!(
        sync.iter()
            .all(|c| c.len() == 6 && c.chars().all(|d| d.is_ascii_digit()))
    );
    assert_eq!(pair.len(), 8);
    let distinct: std::collections::BTreeSet<&String> = sync.iter().collect();
    assert!(distinct.len() > 40);
}

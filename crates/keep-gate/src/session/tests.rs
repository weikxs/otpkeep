use super::{HEARTBEAT_TIMEOUT_MS, IDLE_MS, LINK_TTL_MS, SessionError, Sessions, Token};

const T0: u64 = 1_000_000;

#[test]
fn tokens_are_random_hex_and_never_printed() {
    let a = Token::random().unwrap();
    let b = Token::random().unwrap();

    assert_eq!(a.as_str().len(), 64);
    assert!(a.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
    assert_eq!(format!("{a:?}"), "Token(…)");
}

#[test]
fn a_token_matches_only_itself() {
    let token = Token::random().unwrap();

    assert!(token.matches(token.as_str()));
    assert!(!token.matches(&token.as_str()[1..]));
    assert!(!token.matches(""));
}

#[test]
fn a_link_opens_one_session_once() {
    let mut sessions = Sessions::default();
    let link = sessions.issue_link(T0).unwrap();

    let session = sessions.redeem(link.as_str(), T0 + 1000).unwrap();

    assert!(sessions.check(session.as_str(), T0 + 2000, true).is_ok());
    assert_eq!(
        sessions.redeem(link.as_str(), T0 + 3000),
        Err(SessionError::LinkUsed)
    );
}

#[test]
fn a_late_or_unknown_link_is_refused() {
    let mut sessions = Sessions::default();
    let link = sessions.issue_link(T0).unwrap();

    assert_eq!(sessions.redeem("nope", T0), Err(SessionError::LinkExpired));
    assert_eq!(
        sessions.redeem(link.as_str(), T0 + LINK_TTL_MS + 1),
        Err(SessionError::LinkExpired)
    );
}

#[test]
fn a_new_link_replaces_the_old_one_and_its_session_replaces_the_old_session() {
    let mut sessions = Sessions::default();
    let first = sessions.issue_link(T0).unwrap();
    let old = sessions.redeem(first.as_str(), T0).unwrap();
    let second = sessions.issue_link(T0 + 10).unwrap();

    let new = sessions.redeem(second.as_str(), T0 + 20).unwrap();

    assert_eq!(
        sessions.check(old.as_str(), T0 + 30, false),
        Err(SessionError::NoSession)
    );
    assert!(sessions.check(new.as_str(), T0 + 30, false).is_ok());
}

#[test]
fn a_wrong_bearer_is_refused() {
    let mut sessions = Sessions::default();
    let link = sessions.issue_link(T0).unwrap();
    sessions.redeem(link.as_str(), T0).unwrap();

    assert_eq!(
        sessions.check(link.as_str(), T0, true),
        Err(SessionError::NoSession)
    );
}

#[test]
fn a_session_ends_without_heartbeats() {
    let mut sessions = Sessions::default();
    let link = sessions.issue_link(T0).unwrap();
    let session = sessions.redeem(link.as_str(), T0).unwrap();

    assert_eq!(
        sessions.check(session.as_str(), T0 + HEARTBEAT_TIMEOUT_MS + 1, false),
        Err(SessionError::NoSession)
    );
}

#[test]
fn heartbeats_alone_do_not_keep_an_idle_session() {
    let mut sessions = Sessions::default();
    let link = sessions.issue_link(T0).unwrap();
    let session = sessions.redeem(link.as_str(), T0).unwrap();
    let beat = HEARTBEAT_TIMEOUT_MS / 3;

    let mut now = T0;
    while now + beat <= T0 + IDLE_MS {
        now += beat;
        sessions.check(session.as_str(), now, false).unwrap();
    }

    assert_eq!(
        sessions.check(session.as_str(), T0 + IDLE_MS + beat, false),
        Err(SessionError::NoSession)
    );
}

#[test]
fn is_idle_once_the_link_lapses_and_no_session_is_open() {
    let mut sessions = Sessions::default();
    assert!(sessions.is_idle(T0));

    let link = sessions.issue_link(T0).unwrap();
    assert!(!sessions.is_idle(T0 + 1));
    assert!(sessions.is_idle(T0 + LINK_TTL_MS + 1));

    let link_two = sessions.issue_link(T0).unwrap();
    let _ = link;
    sessions.redeem(link_two.as_str(), T0).unwrap();
    assert!(!sessions.is_idle(T0 + 1));
    sessions.end();
    assert!(sessions.is_idle(T0 + 1));
}

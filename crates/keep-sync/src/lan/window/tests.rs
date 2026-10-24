use super::{
    Accept, CODE_ATTEMPTS, CloseReason, DATA_IDLE_MS, HANDSHAKE_MS, LanWindow, PAIR_CONFIRM_MS,
    SESSION_MAX_MS, SYNC_ALLOW_MS, WINDOW_MAX_CONNECTIONS, WINDOW_WAIT_DEFAULT_MS, WindowError,
    WindowKind, WindowStatus,
};

const T0: u64 = 1_000_000;
const S: u64 = 1_000;

fn open(kind: WindowKind) -> LanWindow {
    LanWindow::open(kind, WINDOW_WAIT_DEFAULT_MS, T0).unwrap()
}

fn handle(window: &mut LanWindow, now: u64) -> super::ConnectionId {
    match window.accept(now) {
        Accept::Handle(id) => id,
        other => panic!("expected to handle the connection, got {other:?}"),
    }
}

#[test]
fn the_wait_time_must_be_between_one_and_five_minutes() {
    assert!(LanWindow::open(WindowKind::Sync, 60 * S, T0).is_ok());
    assert!(LanWindow::open(WindowKind::Sync, 300 * S, T0).is_ok());
    assert_eq!(
        LanWindow::open(WindowKind::Sync, 59 * S, T0).err(),
        Some(WindowError::BadWait)
    );
    assert_eq!(
        LanWindow::open(WindowKind::Sync, 301 * S, T0).err(),
        Some(WindowError::BadWait)
    );
}

#[test]
fn an_unused_window_closes_when_its_time_is_up() {
    let mut window = open(WindowKind::Sync);

    assert_eq!(
        window.remaining_ms(T0 + 30 * S),
        WINDOW_WAIT_DEFAULT_MS - 30 * S
    );
    assert_eq!(window.tick(T0 + WINDOW_WAIT_DEFAULT_MS - 1), None);
    assert!(window.is_advertising());
    assert_eq!(
        window.tick(T0 + WINDOW_WAIT_DEFAULT_MS),
        Some(CloseReason::Expired)
    );
    assert!(!window.is_advertising() && !window.is_listening());
    assert_eq!(window.accept(T0 + WINDOW_WAIT_DEFAULT_MS), Accept::Closed);
}

#[test]
fn only_one_connection_at_a_time_and_others_are_told_busy() {
    let mut window = open(WindowKind::Sync);
    let first = handle(&mut window, T0);

    assert_eq!(window.accept(T0 + S), Accept::Busy);
    window.dropped(first, T0 + 2 * S);
    handle(&mut window, T0 + 3 * S);
}

#[test]
fn a_handshake_that_takes_too_long_is_dropped_without_counting_as_a_wrong_code() {
    let mut window = open(WindowKind::Sync);
    handle(&mut window, T0);

    assert_eq!(window.tick(T0 + HANDSHAKE_MS), None);
    assert_eq!(window.status(), WindowStatus::Waiting);
    assert_eq!(window.code_attempts_left(), CODE_ATTEMPTS);
    handle(&mut window, T0 + HANDSHAKE_MS + 1);
}

#[test]
fn three_wrong_codes_close_the_window() {
    let mut window = open(WindowKind::Pair);
    for attempt in 1..CODE_ATTEMPTS {
        let id = handle(&mut window, T0 + u64::from(attempt));
        assert_eq!(window.code_failed(id, T0 + u64::from(attempt)), None);
        assert_eq!(window.code_attempts_left(), CODE_ATTEMPTS - attempt);
    }
    let last = handle(&mut window, T0 + 10);

    assert_eq!(
        window.code_failed(last, T0 + 10),
        Some(CloseReason::CodeLocked)
    );
    assert_eq!(window.accept(T0 + 11), Accept::Closed);
}

#[test]
fn the_eleventh_connection_closes_the_window() {
    let mut window = open(WindowKind::Sync);
    for n in 0..u64::from(WINDOW_MAX_CONNECTIONS) {
        let id = handle(&mut window, T0 + n);
        window.dropped(id, T0 + n);
    }

    assert_eq!(window.accept(T0 + 100), Accept::Closed);
    assert_eq!(
        window.status(),
        WindowStatus::Closed(CloseReason::TooManyConnections)
    );
}

#[test]
fn a_verified_code_turns_the_window_into_one_session_and_stops_advertising() {
    let mut window = open(WindowKind::Sync);
    let late = T0 + WINDOW_WAIT_DEFAULT_MS - 5 * S;
    let id = handle(&mut window, late);

    window.code_passed(id, late + S);

    assert!(!window.is_advertising() && !window.is_listening());
    assert_eq!(window.status(), WindowStatus::AwaitingPerson);
    assert_eq!(
        window.tick(T0 + WINDOW_WAIT_DEFAULT_MS + S),
        None,
        "the wait time no longer applies"
    );
}

#[test]
fn a_sync_request_not_allowed_in_time_closes_the_session() {
    let mut window = open(WindowKind::Sync);
    let id = handle(&mut window, T0);
    window.code_passed(id, T0 + S);

    assert_eq!(window.tick(T0 + S + SYNC_ALLOW_MS - 1), None);
    assert_eq!(
        window.tick(T0 + S + SYNC_ALLOW_MS),
        Some(CloseReason::AllowTimeout)
    );
}

#[test]
fn a_pairing_not_confirmed_in_time_is_cancelled() {
    let mut window = open(WindowKind::Pair);
    let id = handle(&mut window, T0);
    window.code_passed(id, T0 + S);

    assert_eq!(
        window.tick(T0 + S + PAIR_CONFIRM_MS),
        Some(CloseReason::ConfirmTimeout)
    );
}

#[test]
fn the_data_phase_times_out_only_when_idle() {
    let mut window = open(WindowKind::Sync);
    let id = handle(&mut window, T0);
    window.code_passed(id, T0 + S);
    window.person_agreed(T0 + 20 * S);

    window.activity(T0 + 20 * S + DATA_IDLE_MS - 1);
    assert_eq!(window.tick(T0 + 20 * S + DATA_IDLE_MS + 5), None);
    assert_eq!(window.status(), WindowStatus::Exchanging);
    assert_eq!(
        window.tick(T0 + 20 * S + 2 * DATA_IDLE_MS),
        Some(CloseReason::DataIdle)
    );
}

#[test]
fn a_session_never_lasts_longer_than_five_minutes() {
    let mut window = open(WindowKind::Sync);
    let id = handle(&mut window, T0);
    window.code_passed(id, T0 + S);
    window.person_agreed(T0 + 2 * S);
    let mut now = T0 + 2 * S;
    while now < T0 + SESSION_MAX_MS - DATA_IDLE_MS {
        now += DATA_IDLE_MS / 2;
        window.activity(now);
        assert_eq!(window.tick(now), None);
    }

    assert_eq!(
        window.tick(T0 + SESSION_MAX_MS),
        Some(CloseReason::SessionTimeout)
    );
}

#[test]
fn a_finished_or_cancelled_window_never_reopens() {
    let mut done = open(WindowKind::Sync);
    let id = handle(&mut done, T0);
    done.code_passed(id, T0);
    done.person_agreed(T0);
    done.finish(T0 + S);
    let mut cancelled = open(WindowKind::Pair);
    cancelled.cancel(T0);

    assert_eq!(done.status(), WindowStatus::Closed(CloseReason::Done));
    assert_eq!(
        cancelled.status(),
        WindowStatus::Closed(CloseReason::Cancelled)
    );
    assert_eq!(done.accept(T0 + 2 * S), Accept::Closed);
    assert_eq!(
        cancelled.tick(T0 + 2 * S),
        None,
        "a closed window reports its end once"
    );
}

#[test]
fn events_for_another_connection_are_ignored() {
    let mut window = open(WindowKind::Sync);
    let first = handle(&mut window, T0);
    window.dropped(first, T0);
    let second = handle(&mut window, T0 + 1);

    assert_eq!(window.code_failed(first, T0 + 2), None);
    window.code_passed(first, T0 + 2);
    assert_eq!(window.code_attempts_left(), CODE_ATTEMPTS);
    assert_eq!(window.status(), WindowStatus::Handshaking);
    window.code_passed(second, T0 + 3);
    assert_eq!(window.status(), WindowStatus::AwaitingPerson);
}

#[test]
fn keepalives_are_due_every_ten_seconds_while_waiting_for_a_person() {
    let mut window = open(WindowKind::Pair);
    let id = handle(&mut window, T0);
    window.code_passed(id, T0);

    assert!(!window.keepalive_due(T0 + 9 * S));
    assert!(window.keepalive_due(T0 + 10 * S));
    window.keepalive_sent(T0 + 10 * S);
    assert!(!window.keepalive_due(T0 + 15 * S));
    window.person_agreed(T0 + 16 * S);
    assert!(
        !window.keepalive_due(T0 + 40 * S),
        "the data phase sends data instead"
    );
}

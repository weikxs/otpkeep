use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::{Board, Clipboard, Slot, clear_if_ours};

const LONG: Duration = Duration::from_hours(1);
const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(5);

/// A clipboard in memory, shared by every "opening" of it.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Option<String>>>);

impl Board for Fake {
    fn text(&mut self) -> Option<Zeroizing<String>> {
        self.0.lock().unwrap().clone().map(Zeroizing::new)
    }
    fn put(&mut self, code: &str) -> Result<(), arboard::Error> {
        *self.0.lock().unwrap() = Some(code.to_owned());
        Ok(())
    }
    fn clear(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}

impl Fake {
    fn holds(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }

    fn clipboard(&self) -> Clipboard {
        let board = self.clone();
        Clipboard::with(Arc::new(move || {
            Some(Box::new(board.clone()) as Box<dyn Board>)
        }))
    }
}

fn code(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.to_owned())
}

fn eventually(check: impl Fn() -> bool) -> bool {
    let until = Instant::now() + PATIENCE;
    while Instant::now() < until {
        if check() {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

#[test]
fn copying_again_moves_the_deadline() {
    let start = Instant::now();
    let mut slot = Slot::default();
    slot.arm(Some(code("123456")), start + Duration::from_secs(30));
    slot.arm(Some(code("123456")), start + Duration::from_secs(50));

    assert!(slot.take_due(start + Duration::from_secs(30)).is_none());
    assert_eq!(
        slot.take_due(start + Duration::from_secs(50))
            .as_deref()
            .map(String::as_str),
        Some("123456")
    );
    assert!(slot.wait(start).is_none(), "nothing is left to clear");
}

#[test]
fn copying_without_clearing_forgets_the_earlier_code() {
    let start = Instant::now();
    let mut slot = Slot::default();
    slot.arm(Some(code("123456")), start);
    slot.arm(None, start);

    assert!(slot.take_due(start + LONG).is_none());
}

#[test]
fn clears_only_its_own_code() {
    let mut board = Fake::default();
    board.put("something else").unwrap();
    clear_if_ours(&mut board, "123456");
    assert_eq!(board.holds().as_deref(), Some("something else"));

    board.put("123456").unwrap();
    clear_if_ours(&mut board, "123456");
    assert_eq!(board.holds(), None);
}

#[test]
fn a_due_code_is_cleared_by_the_worker() {
    let board = Fake::default();
    let clipboard = board.clipboard();

    clipboard
        .copy_then_clear(code("123456"), Some(Duration::ZERO))
        .unwrap();

    assert!(eventually(|| board.holds().is_none()));
}

#[test]
fn quitting_clears_the_pending_code_at_once() {
    let board = Fake::default();
    let clipboard = board.clipboard();
    clipboard
        .copy_then_clear(code("123456"), Some(LONG))
        .unwrap();
    assert_eq!(board.holds().as_deref(), Some("123456"));

    clipboard.clear_now();

    assert_eq!(board.holds(), None);
}

#[test]
fn quitting_leaves_a_code_the_person_keeps() {
    let board = Fake::default();
    let clipboard = board.clipboard();
    clipboard.copy_then_clear(code("123456"), None).unwrap();

    clipboard.clear_now();

    assert_eq!(board.holds().as_deref(), Some("123456"));
}

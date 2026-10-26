//! Copying a code: write it to the clipboard, keep it out of clipboard history apps, and clear it later if it
//! is still there (PRD FR-1.3; design/12 §6).
//!
//! One clear is pending at a time, for the code copied last: copying again moves its deadline, so copying
//! the same code twice keeps it for the full time after the second copy. When the app quits, the pending
//! code is cleared at once rather than left behind.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

/// The system clipboard, or a stand-in in tests.
pub trait Board {
    /// The text on the clipboard, when there is text.
    fn text(&mut self) -> Option<Zeroizing<String>>;
    /// Writes `code`, kept out of clipboard history where the system allows it.
    ///
    /// # Errors
    ///
    /// Returns the clipboard's error.
    fn put(&mut self, code: &str) -> Result<(), arboard::Error>;
    fn clear(&mut self);
}

/// Opens the clipboard each time it is needed; `None` when it cannot be opened.
pub type OpenBoard = Arc<dyn Fn() -> Option<Box<dyn Board>> + Send + Sync>;

/// The code to clear and when.
struct Pending {
    code: Zeroizing<String>,
    due: Instant,
}

#[derive(Default)]
struct Slot {
    pending: Option<Pending>,
    /// The thread that clears has been started.
    worker: bool,
}

impl Slot {
    /// Remembers `code` to clear at `due`, replacing any earlier one; `None` forgets it (clearing is off).
    fn arm(&mut self, code: Option<Zeroizing<String>>, due: Instant) {
        self.pending = code.map(|code| Pending { code, due });
    }

    /// The pending code when its time has come.
    fn take_due(&mut self, now: Instant) -> Option<Zeroizing<String>> {
        if self.pending.as_ref().is_some_and(|p| p.due <= now) {
            return self.pending.take().map(|p| p.code);
        }
        None
    }

    /// How long until the pending code is due; `None` with nothing pending.
    fn wait(&self, now: Instant) -> Option<Duration> {
        self.pending
            .as_ref()
            .map(|p| p.due.saturating_duration_since(now))
    }
}

/// Clears `code` unless something else was copied since.
fn clear_if_ours(board: &mut dyn Board, code: &str) {
    if board.text().is_some_and(|text| text.as_str() == code) {
        board.clear();
    }
}

/// Copies codes and clears them later.
pub struct Clipboard {
    shared: Arc<(Mutex<Slot>, Condvar)>,
    open: OpenBoard,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::with(Arc::new(|| {
            arboard::Clipboard::new()
                .ok()
                .map(|c| Box::new(SystemBoard(c)) as Box<dyn Board>)
        }))
    }
}

impl Clipboard {
    /// A clipboard opened by `open`.
    pub fn with(open: OpenBoard) -> Self {
        Self {
            shared: Arc::new((Mutex::new(Slot::default()), Condvar::new())),
            open,
        }
    }

    fn slot(&self) -> MutexGuard<'_, Slot> {
        self.shared.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts `code` on the clipboard and clears it after `after`, unless something else was copied meanwhile;
    /// with `None` it stays (the person turned clearing off).
    ///
    /// # Errors
    ///
    /// Returns the clipboard's error when the code could not be written.
    pub fn copy_then_clear(
        &self,
        code: Zeroizing<String>,
        after: Option<Duration>,
    ) -> Result<(), arboard::Error> {
        let mut board = (self.open)().ok_or(arboard::Error::ClipboardNotSupported)?;
        board.put(&code)?;
        let now = Instant::now();
        let mut slot = self.slot();
        slot.arm(after.map(|_| code), now + after.unwrap_or_default());
        if slot.pending.is_some() && !slot.worker {
            slot.worker = self.start_worker();
        }
        drop(slot);
        self.shared.1.notify_all();
        Ok(())
    }

    /// Clears the pending code now if the clipboard still holds it; for when the app quits.
    pub fn clear_now(&self) {
        let pending = self.slot().pending.take();
        if let (Some(pending), Some(mut board)) = (pending, (self.open)()) {
            clear_if_ours(board.as_mut(), &pending.code);
        }
    }

    /// Starts the one thread that clears codes when they are due; whether it runs.
    fn start_worker(&self) -> bool {
        let shared = Arc::clone(&self.shared);
        let open = Arc::clone(&self.open);
        std::thread::Builder::new()
            .name("clipboard-clear".to_owned())
            .spawn(move || clear_when_due(&shared, &open))
            .is_ok()
    }
}

/// Waits for each pending code's deadline and clears it; runs for the life of the app.
fn clear_when_due(shared: &(Mutex<Slot>, Condvar), open: &OpenBoard) {
    let (lock, ready) = shared;
    let mut slot = lock.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        if let Some(code) = slot.take_due(Instant::now()) {
            drop(slot);
            if let Some(mut board) = open() {
                clear_if_ours(board.as_mut(), &code);
            }
            slot = lock.lock().unwrap_or_else(PoisonError::into_inner);
            continue;
        }
        slot = match slot.wait(Instant::now()) {
            Some(wait) => {
                ready
                    .wait_timeout(slot, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => ready.wait(slot).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

struct SystemBoard(arboard::Clipboard);

impl Board for SystemBoard {
    fn text(&mut self) -> Option<Zeroizing<String>> {
        self.0.get_text().ok().map(Zeroizing::new)
    }

    fn put(&mut self, code: &str) -> Result<(), arboard::Error> {
        write_hidden(&mut self.0, code)
    }

    fn clear(&mut self) {
        let _ = self.0.clear();
    }
}

#[cfg(target_os = "macos")]
fn write_hidden(clipboard: &mut arboard::Clipboard, code: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtApple;
    clipboard.set().exclude_from_history().text(code)
}

#[cfg(target_os = "windows")]
fn write_hidden(clipboard: &mut arboard::Clipboard, code: &str) -> Result<(), arboard::Error> {
    use arboard::SetExtWindows;
    clipboard
        .set()
        .exclude_from_monitoring()
        .exclude_from_cloud()
        .exclude_from_history()
        .text(code)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_hidden(clipboard: &mut arboard::Clipboard, code: &str) -> Result<(), arboard::Error> {
    clipboard.set_text(code)
}

#[cfg(test)]
mod tests;

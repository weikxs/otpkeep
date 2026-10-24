//! A pairing or sync window: who may connect, for how long, and when it closes (design/06-sync.md §3.1–3.3).
//!
//! The window is a plain state machine. The network layer reports what happens with the current time in
//! milliseconds from a monotonic clock, and asks [`LanWindow::tick`] regularly; the window decides. One
//! connection is handled at a time; a verified code turns the window into that connection's session and
//! stops advertising and listening; wrong codes and connections are counted per window; a closed window
//! never reopens.

/// Default `WINDOW_WAIT` (design/10-reference.md §2).
pub const WINDOW_WAIT_DEFAULT_MS: u64 = 2 * 60 * 1000;
/// Shortest `WINDOW_WAIT` a user may choose.
pub const WINDOW_WAIT_MIN_MS: u64 = 60 * 1000;
/// Longest `WINDOW_WAIT` a user may choose.
pub const WINDOW_WAIT_MAX_MS: u64 = 5 * 60 * 1000;
/// `HANDSHAKE`: from accepting a connection to a verified code.
pub const HANDSHAKE_MS: u64 = 10 * 1000;
/// `PAIR_CONFIRM`: from a verified pairing code until both people confirm the check code.
pub const PAIR_CONFIRM_MS: u64 = 60 * 1000;
/// `SYNC_ALLOW`: from a verified sync code until the person at the window allows the request.
pub const SYNC_ALLOW_MS: u64 = 30 * 1000;
/// `DATA_IDLE`: the longest silence while exchanging data.
pub const DATA_IDLE_MS: u64 = 30 * 1000;
/// `SESSION_MAX`: from accepting the connection to the end of the session.
pub const SESSION_MAX_MS: u64 = 5 * 60 * 1000;
/// `CODE_ATTEMPTS`: wrong codes per window.
pub const CODE_ATTEMPTS: u8 = 3;
/// `WINDOW_MAX_CONNECTIONS`: connections per window.
pub const WINDOW_MAX_CONNECTIONS: u8 = 10;
/// How often each side sends a keepalive while waiting for a person.
pub const KEEPALIVE_MS: u64 = 10 * 1000;

/// What the window is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// Adding a device.
    Pair,
    /// Syncing with a member.
    Sync,
}

/// Why a window could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WindowError {
    /// The wait time is outside one to five minutes.
    #[error("the window time must be between one and five minutes")]
    BadWait,
}

/// Why a window closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Nobody completed a code in time (`WINDOW_EXPIRED`).
    Expired,
    /// Too many wrong codes (`CODE_LOCKED`).
    CodeLocked,
    /// Too many connections.
    TooManyConnections,
    /// The pairing check code was not confirmed in time (`PAIR_CONFIRM_TIMEOUT`).
    ConfirmTimeout,
    /// The sync request was not allowed in time (`ALLOW_TIMEOUT`).
    AllowTimeout,
    /// The data exchange went silent (`SESSION_TIMEOUT`).
    DataIdle,
    /// The session ran too long (`SESSION_TIMEOUT`).
    SessionTimeout,
    /// The session finished.
    Done,
    /// Someone closed it, declined, or the session failed.
    Cancelled,
}

/// Where the window stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowStatus {
    /// Advertising and listening.
    Waiting,
    /// Handling one connection's handshake and code.
    Handshaking,
    /// The code was right; waiting for the people (check code or "allow").
    AwaitingPerson,
    /// Exchanging data.
    Exchanging,
    /// Closed for good.
    Closed(CloseReason),
}

/// Identifies one accepted connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionId(u8);

/// What to do with a new connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// Handle it.
    Handle(ConnectionId),
    /// Another connection is being handled: reply `BUSY` and disconnect.
    Busy,
    /// The window is closed: disconnect.
    Closed,
}

/// One pairing or sync window.
#[derive(Debug, Clone)]
pub struct LanWindow {
    kind: WindowKind,
    opened_ms: u64,
    wait_ms: u64,
    status: WindowStatus,
    current: Option<ConnectionId>,
    accepted_ms: u64,
    phase_ms: u64,
    last_activity_ms: u64,
    last_keepalive_ms: u64,
    connections: u8,
    wrong_codes: u8,
}

impl LanWindow {
    /// Opens a window at `now_ms` that waits `wait_ms` for a connection to get the code right.
    ///
    /// # Errors
    ///
    /// Returns [`WindowError::BadWait`] for a wait outside one to five minutes.
    pub fn open(kind: WindowKind, wait_ms: u64, now_ms: u64) -> Result<Self, WindowError> {
        if !(WINDOW_WAIT_MIN_MS..=WINDOW_WAIT_MAX_MS).contains(&wait_ms) {
            return Err(WindowError::BadWait);
        }
        Ok(Self {
            kind,
            opened_ms: now_ms,
            wait_ms,
            status: WindowStatus::Waiting,
            current: None,
            accepted_ms: now_ms,
            phase_ms: now_ms,
            last_activity_ms: now_ms,
            last_keepalive_ms: now_ms,
            connections: 0,
            wrong_codes: 0,
        })
    }

    /// The window's kind.
    #[must_use]
    pub fn kind(&self) -> WindowKind {
        self.kind
    }

    /// Where the window stands.
    #[must_use]
    pub fn status(&self) -> WindowStatus {
        self.status
    }

    /// Whether the window should be announced on the network.
    #[must_use]
    pub fn is_advertising(&self) -> bool {
        matches!(
            self.status,
            WindowStatus::Waiting | WindowStatus::Handshaking
        )
    }

    /// Whether the listening socket should be open.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.is_advertising()
    }

    /// Time left until the window expires, for the countdown and the announcement.
    #[must_use]
    pub fn remaining_ms(&self, now_ms: u64) -> u64 {
        (self.opened_ms + self.wait_ms).saturating_sub(now_ms)
    }

    /// Wrong codes still allowed in this window.
    #[must_use]
    pub fn code_attempts_left(&self) -> u8 {
        CODE_ATTEMPTS - self.wrong_codes
    }

    /// A connection arrived.
    pub fn accept(&mut self, now_ms: u64) -> Accept {
        match self.status {
            WindowStatus::Waiting => {}
            WindowStatus::Closed(_) => return Accept::Closed,
            _ => return Accept::Busy,
        }
        if self.connections >= WINDOW_MAX_CONNECTIONS {
            self.close(CloseReason::TooManyConnections);
            return Accept::Closed;
        }
        self.connections += 1;
        let id = ConnectionId(self.connections);
        self.current = Some(id);
        self.accepted_ms = now_ms;
        self.status = WindowStatus::Handshaking;
        Accept::Handle(id)
    }

    /// The connection ended before its code was verified, for a reason other than a wrong code (protocol
    /// error, not a member, disconnect). It does not count as a wrong code.
    pub fn dropped(&mut self, id: ConnectionId, _now_ms: u64) {
        if self.is_current(id) && self.status == WindowStatus::Handshaking {
            self.current = None;
            self.status = WindowStatus::Waiting;
        }
    }

    /// The connection's code was wrong. Returns the reason when the window closes because of it.
    pub fn code_failed(&mut self, id: ConnectionId, now_ms: u64) -> Option<CloseReason> {
        if !self.is_current(id) || self.status != WindowStatus::Handshaking {
            return None;
        }
        self.wrong_codes += 1;
        if self.wrong_codes >= CODE_ATTEMPTS {
            return self.close(CloseReason::CodeLocked);
        }
        self.dropped(id, now_ms);
        None
    }

    /// The connection's code was right: the window becomes its session and stops advertising.
    pub fn code_passed(&mut self, id: ConnectionId, now_ms: u64) {
        if self.is_current(id) && self.status == WindowStatus::Handshaking {
            self.status = WindowStatus::AwaitingPerson;
            self.phase_ms = now_ms;
            self.last_keepalive_ms = now_ms;
        }
    }

    /// Both people confirmed (pairing) or the person allowed the request (sync): data may flow.
    pub fn person_agreed(&mut self, now_ms: u64) {
        if self.status == WindowStatus::AwaitingPerson {
            self.status = WindowStatus::Exchanging;
            self.last_activity_ms = now_ms;
        }
    }

    /// Data moved in the data phase.
    pub fn activity(&mut self, now_ms: u64) {
        if self.status == WindowStatus::Exchanging {
            self.last_activity_ms = now_ms;
        }
    }

    /// Whether a keepalive should be sent now.
    #[must_use]
    pub fn keepalive_due(&self, now_ms: u64) -> bool {
        self.status == WindowStatus::AwaitingPerson
            && now_ms.saturating_sub(self.last_keepalive_ms) >= KEEPALIVE_MS
    }

    /// A keepalive was sent.
    pub fn keepalive_sent(&mut self, now_ms: u64) {
        self.last_keepalive_ms = now_ms;
    }

    /// The session finished.
    pub fn finish(&mut self, _now_ms: u64) {
        self.close(CloseReason::Done);
    }

    /// Closed by a person or because the session failed.
    pub fn cancel(&mut self, _now_ms: u64) {
        self.close(CloseReason::Cancelled);
    }

    /// Applies the timers. Returns the reason when the window closes now. A handshake that runs too long is
    /// dropped without closing the window.
    pub fn tick(&mut self, now_ms: u64) -> Option<CloseReason> {
        let since = |start: u64| now_ms.saturating_sub(start);
        let session_over = since(self.accepted_ms) >= SESSION_MAX_MS;
        match self.status {
            WindowStatus::Waiting | WindowStatus::Handshaking
                if since(self.opened_ms) >= self.wait_ms =>
            {
                self.close(CloseReason::Expired)
            }
            WindowStatus::Closed(_) | WindowStatus::Waiting => None,
            WindowStatus::Handshaking => {
                if since(self.accepted_ms) >= HANDSHAKE_MS {
                    self.current = None;
                    self.status = WindowStatus::Waiting;
                }
                None
            }
            WindowStatus::AwaitingPerson | WindowStatus::Exchanging if session_over => {
                self.close(CloseReason::SessionTimeout)
            }
            WindowStatus::AwaitingPerson => {
                let (limit, reason) = match self.kind {
                    WindowKind::Pair => (PAIR_CONFIRM_MS, CloseReason::ConfirmTimeout),
                    WindowKind::Sync => (SYNC_ALLOW_MS, CloseReason::AllowTimeout),
                };
                (since(self.phase_ms) >= limit)
                    .then(|| self.close(reason))
                    .flatten()
            }
            WindowStatus::Exchanging => (since(self.last_activity_ms) > DATA_IDLE_MS)
                .then(|| self.close(CloseReason::DataIdle))
                .flatten(),
        }
    }

    fn is_current(&self, id: ConnectionId) -> bool {
        self.current == Some(id)
    }

    fn close(&mut self, reason: CloseReason) -> Option<CloseReason> {
        if matches!(self.status, WindowStatus::Closed(_)) {
            return None;
        }
        self.status = WindowStatus::Closed(reason);
        self.current = None;
        tracing::info!(?reason, "window closed");
        Some(reason)
    }
}

#[cfg(test)]
mod tests;

//! One-time links and the single browser session (design/07-platform.md §2.5).
//!
//! The link token travels after `#` in the URL, so it never reaches request lines, logs or `Referer`; the page
//! trades it for a session token, which every API request then carries in `Authorization: Bearer`.

use std::fmt::Write as _;

/// How long a link may wait before it is opened (`WEB_LINK_TTL`).
pub const LINK_TTL_MS: u64 = 60 * 1000;
/// A session ends this long after the last heartbeat (`WEB_HEARTBEAT_TIMEOUT`).
pub const HEARTBEAT_TIMEOUT_MS: u64 = 45 * 1000;
/// A session ends this long after the last action by the person (`WEB_IDLE`).
pub const IDLE_MS: u64 = 15 * 60 * 1000;
const TOKEN_BYTES: usize = 32;

/// A 256-bit random token, as 64 hex digits.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// A new random token.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Random`] when the system random source fails.
    pub fn random() -> Result<Self, SessionError> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| SessionError::Random)?;
        let mut text = String::with_capacity(TOKEN_BYTES * 2);
        for byte in bytes {
            let _ = write!(text, "{byte:02x}");
        }
        Ok(Self(text))
    }

    /// The token's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares with `other` in time that does not depend on where they differ.
    #[must_use]
    pub fn matches(&self, other: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), other.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(…)")
    }
}

/// Why a link or session was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// The link was already opened.
    #[error("this link has already been used")]
    LinkUsed,
    /// The link is older than [`LINK_TTL_MS`], or unknown.
    #[error("this link has expired; open the page again from OTPKeep")]
    LinkExpired,
    /// No session, a wrong token, or the session ended.
    #[error("the session has ended; open the page again from OTPKeep")]
    NoSession,
    /// The system random source failed.
    #[error("the system random number generator is unavailable")]
    Random,
}

#[derive(Debug)]
struct Link {
    token: Token,
    issued_ms: u64,
    used: bool,
}

#[derive(Debug)]
struct Session {
    token: Token,
    last_beat_ms: u64,
    last_action_ms: u64,
}

/// The link and session state of the page server.
#[derive(Debug, Default)]
pub struct Sessions {
    link: Option<Link>,
    session: Option<Session>,
}

impl Sessions {
    /// Issues a new link, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Random`] when no token can be made.
    pub fn issue_link(&mut self, now_ms: u64) -> Result<Token, SessionError> {
        let token = Token::random()?;
        self.link = Some(Link {
            token: token.clone(),
            issued_ms: now_ms,
            used: false,
        });
        Ok(token)
    }

    /// Trades a link token for a new session; the link then stops working and any earlier session ends.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] for a used, expired or unknown link.
    pub fn redeem(&mut self, link: &str, now_ms: u64) -> Result<Token, SessionError> {
        let current = self
            .link
            .as_mut()
            .filter(|l| l.token.matches(link))
            .ok_or(SessionError::LinkExpired)?;
        if current.used {
            return Err(SessionError::LinkUsed);
        }
        if now_ms.saturating_sub(current.issued_ms) > LINK_TTL_MS {
            return Err(SessionError::LinkExpired);
        }
        current.used = true;
        let token = Token::random()?;
        self.session = Some(Session {
            token: token.clone(),
            last_beat_ms: now_ms,
            last_action_ms: now_ms,
        });
        Ok(token)
    }

    /// Checks a request's bearer token. A heartbeat keeps the session alive; any other request also counts
    /// as an action by the person.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NoSession`] when the token is wrong or the session has ended.
    pub fn check(
        &mut self,
        bearer: &str,
        now_ms: u64,
        is_action: bool,
    ) -> Result<(), SessionError> {
        self.expire(now_ms);
        let session = self
            .session
            .as_mut()
            .filter(|s| s.token.matches(bearer))
            .ok_or(SessionError::NoSession)?;
        session.last_beat_ms = now_ms;
        if is_action {
            session.last_action_ms = now_ms;
        }
        Ok(())
    }

    /// Ends the session, if any.
    pub fn end(&mut self) {
        self.session = None;
    }

    /// Whether nothing can use the server any more: no live session and no link waiting to be opened.
    #[must_use]
    pub fn is_idle(&mut self, now_ms: u64) -> bool {
        self.expire(now_ms);
        let link_waiting = self
            .link
            .as_ref()
            .is_some_and(|l| !l.used && now_ms.saturating_sub(l.issued_ms) <= LINK_TTL_MS);
        self.session.is_none() && !link_waiting
    }

    fn expire(&mut self, now_ms: u64) {
        let ended = self.session.as_ref().is_some_and(|s| {
            now_ms.saturating_sub(s.last_beat_ms) > HEARTBEAT_TIMEOUT_MS
                || now_ms.saturating_sub(s.last_action_ms) > IDLE_MS
        });
        if ended {
            self.session = None;
        }
    }
}

#[cfg(test)]
mod tests;

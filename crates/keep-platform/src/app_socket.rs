//! The local socket through which the running app gives codes to scripts (the `otpkeep` command line), so they
//! work while the app holds the vault (docs/scripting.md).
//!
//! The socket is `app.sock` in the data directory: the directory is `0700` and the socket `0600`, so only the
//! user who runs the app can connect. One request per connection, each a single line of JSON of at most
//! [`MAX_LINE`] bytes, answered with one line of JSON. Messages carry the protocol version `"v": 1`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The socket's file name in the data directory.
pub const SOCKET_FILE: &str = "app.sock";
/// The protocol version in every message.
pub const VERSION: u32 = 1;
/// The longest request or response line, in bytes (the line break included).
pub const MAX_LINE: usize = 4096;
/// How long either side waits for the other to send or receive.
pub const TIMEOUT: Duration = Duration::from_secs(2);
/// Mode of the data directory: only its owner may enter it.
const DIR_MODE: u32 = 0o700;
/// Mode of the socket: only its owner may connect.
const SOCKET_MODE: u32 = 0o600;

/// The socket's path in `data_dir`.
#[must_use]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOCKET_FILE)
}

/// What a script asks: the current code of the one account `query` names; with `next`, a counter-based
/// account's next code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// The protocol version, [`VERSION`].
    pub v: u32,
    /// What to do; only `code` exists.
    pub op: String,
    /// The search naming one account.
    pub query: String,
    /// Make a counter-based (HOTP) account's next code.
    #[serde(default)]
    pub next: bool,
}

impl Request {
    /// A request for the code of the account `query` names.
    #[must_use]
    pub fn code(query: &str, next: bool) -> Self {
        Self {
            v: VERSION,
            op: "code".to_owned(),
            query: query.to_owned(),
            next,
        }
    }
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    /// The code.
    Code(Code),
    /// Why there is no code.
    Error(Failure),
}

/// A code given to a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    /// The protocol version, [`VERSION`].
    pub v: u32,
    /// Always `true`.
    pub ok: bool,
    /// The account's name.
    pub name: String,
    /// The code.
    pub code: String,
    /// Seconds the code stays valid; absent for counter-based accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_for_s: Option<u16>,
}

/// Why no code was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// The protocol version, [`VERSION`].
    pub v: u32,
    /// Always `false`.
    pub ok: bool,
    /// A stable code: `LOCKED`, `NOT_FOUND`, `AMBIGUOUS`, `NEEDS_NEXT`, `DISABLED`, `UNAVAILABLE`, `READ_ONLY`,
    /// `BAD_REQUEST` or `FAILED`.
    pub error: String,
    /// What went wrong and what to do, in English.
    pub message: String,
    /// The accounts a search matched, for `AMBIGUOUS`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
}

impl Response {
    /// A code.
    #[must_use]
    pub fn code(name: &str, code: String, valid_for_s: Option<u16>) -> Self {
        Self::Code(Code {
            v: VERSION,
            ok: true,
            name: name.to_owned(),
            code,
            valid_for_s,
        })
    }

    /// A failure with its stable code, message and candidates.
    #[must_use]
    pub fn error(error: &str, message: &str, candidates: Vec<String>) -> Self {
        Self::Error(Failure {
            v: VERSION,
            ok: false,
            error: error.to_owned(),
            message: message.to_owned(),
            candidates,
        })
    }
}

/// Why a message could not be read.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    /// The line was longer than [`MAX_LINE`].
    #[error("the message is too long")]
    TooLong,
    /// Not the JSON of a message, or another protocol version.
    #[error("the message is not understood")]
    Invalid,
    /// The connection failed or timed out.
    #[error("the connection failed")]
    Io(#[from] io::Error),
}

/// Reads one line of at most [`MAX_LINE`] bytes and parses it.
///
/// # Errors
///
/// Returns [`MessageError`] for a line too long, one that is not a message of this version, or a failed read.
pub fn read_message<T: for<'de> Deserialize<'de>>(input: impl Read) -> Result<T, MessageError> {
    let mut line = Vec::new();
    let limit = u64::try_from(MAX_LINE).unwrap_or(u64::MAX) + 1;
    BufReader::new(input.take(limit)).read_until(b'\n', &mut line)?;
    if line.len() > MAX_LINE {
        return Err(MessageError::TooLong);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&line).map_err(|_| MessageError::Invalid)?;
    if value.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(VERSION)) {
        return Err(MessageError::Invalid);
    }
    serde_json::from_value(value).map_err(|_| MessageError::Invalid)
}

/// Writes `message` as one line.
///
/// # Errors
///
/// Returns the [`io::Error`] of a failed write.
pub fn write_message(mut output: impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    output.write_all(&line)?;
    output.flush()
}

/// Why the app could not be asked.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    /// No app is listening (no socket, or one left behind).
    #[error("the app is not running")]
    NotRunning,
    /// The app is there but the exchange failed.
    #[error("the app did not answer: {0}")]
    Failed(#[from] MessageError),
}

/// Asks the app listening in `data_dir`.
///
/// # Errors
///
/// Returns [`AskError::NotRunning`] when no app listens there, [`AskError::Failed`] when it does not answer.
pub fn ask(data_dir: &Path, request: &Request) -> Result<Response, AskError> {
    let mut stream =
        UnixStream::connect(socket_path(data_dir)).map_err(|_| AskError::NotRunning)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .map_err(MessageError::Io)?;
    write_message(&mut stream, request).map_err(MessageError::Io)?;
    Ok(read_message(&stream)?)
}

/// Listens on the socket in `data_dir`: the directory is made private (`0700`), a socket left behind by an app
/// that ended is removed, and the new socket is private (`0600`).
///
/// # Errors
///
/// Returns [`io::ErrorKind::AddrInUse`] when another app answers on the socket, or the error of a step that
/// failed.
pub fn listen(data_dir: &Path) -> io::Result<UnixListener> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(DIR_MODE))?;
    let path = socket_path(data_dir);
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err(io::Error::from(io::ErrorKind::AddrInUse));
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(SOCKET_MODE))?;
    Ok(listener)
}

/// Removes the socket when the app quits.
pub fn remove(data_dir: &Path) {
    let _ = std::fs::remove_file(socket_path(data_dir));
}

#[cfg(test)]
mod tests;

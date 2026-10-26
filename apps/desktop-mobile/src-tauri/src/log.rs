//! A small file log for warnings and errors, so a problem a person reports can be looked at afterwards. It
//! stays bounded (one file of at most 1 MB plus one older file) under the app's log folder, holds only what
//! the code logs at warning level or above, and the code never logs secrets, codes, names or paths
//! (standards §5). Events before the folder is known are kept in memory, a few at most, and written then.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// The log file's name; the older one adds `.1`.
const FILE_NAME: &str = "otpkeep.log";
/// A log file is started afresh when it would grow past this.
pub const MAX_BYTES: u64 = 1024 * 1024;
/// Lines kept until the folder is known.
const EARLY_LINES: usize = 32;
/// Owner read and write only.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;

/// Where the log goes; shared by the subscriber and the app that names the folder.
#[derive(Clone)]
pub struct Log(Arc<Mutex<Sink>>);

#[derive(Default)]
struct Sink {
    file: Option<LogFile>,
    early: Vec<String>,
}

impl Log {
    /// Logs warnings and errors of the whole app from now on; the folder comes later with [`Log::attach`].
    #[must_use]
    pub fn install() -> Self {
        let log = Self(Arc::default());
        if tracing::subscriber::set_global_default(log.clone()).is_err() {
            // Another subscriber was set (a test harness): the app runs without the file log.
        }
        log
    }

    /// Starts writing to `dir`, with what was logged so far.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the folder cannot be made.
    pub fn attach(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut sink = self.sink();
        let mut file = LogFile::new(dir.join(FILE_NAME), MAX_BYTES);
        for line in sink.early.drain(..) {
            file.write_line(&line);
        }
        sink.file = Some(file);
        Ok(())
    }

    /// Starts writing to the app's log folder; when it cannot be used, warnings are not kept.
    pub fn attach_to(&self, app: &tauri::AppHandle) {
        use tauri::Manager;
        let attached = app
            .path()
            .app_log_dir()
            .map_err(|_| ())
            .and_then(|dir| self.attach(&dir).map_err(|_| ()));
        if attached.is_err() {
            tracing::warn!("the log folder cannot be used; warnings are not kept");
        }
    }

    fn sink(&self) -> std::sync::MutexGuard<'_, Sink> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, line: String) {
        let mut sink = self.sink();
        if let Some(file) = sink.file.as_mut() {
            file.write_line(&line);
        } else if sink.early.len() < EARLY_LINES {
            sink.early.push(line);
        }
    }
}

impl Subscriber for Log {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.is_event() && *metadata.level() <= Level::WARN
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::WARN)
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        // Spans are not followed; every one gets the same id.
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        self.write(line_of(event, unix_seconds()));
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// `<unix seconds> <LEVEL> <target>: <message> <field>=<value>…`
fn line_of(event: &Event<'_>, seconds: u64) -> String {
    let mut fields = Fields::default();
    event.record(&mut fields);
    let metadata = event.metadata();
    format!(
        "{seconds} {} {}: {}{}\n",
        metadata.level(),
        metadata.target(),
        fields.message,
        fields.rest
    )
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.rest, " {}={value:?}", field.name());
        }
    }
}

/// One log file and its older copy; never more than `max` bytes each.
struct LogFile {
    path: PathBuf,
    max: u64,
}

impl LogFile {
    const fn new(path: PathBuf, max: u64) -> Self {
        Self { path, max }
    }

    /// Appends `line`, moving the file aside first when it would grow past the limit. A failure is dropped:
    /// the log must never stop the app.
    fn write_line(&mut self, line: &str) {
        let size = std::fs::metadata(&self.path).map_or(0, |m| m.len());
        let line = clip(line, self.max);
        if size > 0 && size + line.len() as u64 > self.max {
            let mut older = self.path.clone().into_os_string();
            older.push(".1");
            let _ = std::fs::rename(&self.path, older);
        }
        if let Ok(mut file) = open_append(&self.path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

/// A line longer than the whole file allows is cut, at a character boundary.
fn clip(line: &str, max: u64) -> &str {
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    if line.len() <= max {
        return line;
    }
    let mut end = max;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

fn open_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_MODE);
    }
    options.open(path)
}

#[cfg(test)]
mod tests;

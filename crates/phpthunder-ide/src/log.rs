//! Local error log for debugging: `~/.config/phpthunder/errors.log`, one
//! line per entry (`<UTC time> <LEVEL> <file:line> <message>`), plus panics
//! with their backtrace. Every entry is mirrored on stderr. When the file
//! passes [`MAX_BYTES`] it is moved to `errors.log.1`, so at most two files
//! are kept. Snapshots and tests never write it (see `settings`).

use std::fmt::Display;
use std::io::Write;
use std::panic::Location;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Log file name inside the config folder.
const FILE: &str = "errors.log";

/// Size after which the log is rotated to `errors.log.1`.
const MAX_BYTES: u64 = 1024 * 1024;

/// Severity of an entry.
#[derive(Debug, Clone, Copy)]
enum Level {
    /// Something failed and the user may have lost work or a feature.
    Error,
    /// Something was ignored or fell back to a default.
    Warn,
    /// The program panicked.
    Panic,
}

impl Level {
    /// Upper-case label written in the file.
    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Panic => "PANIC",
        }
    }
}

/// Logs a failure, tagged with the caller's `file:line`. Used where an
/// operation fails (save, git, file ops, settings), next to the notice the
/// user sees.
#[track_caller]
pub fn error(message: impl Display) {
    write(Level::Error, Location::caller(), &message.to_string());
}

/// Logs a recoverable problem (invalid settings, a fallback taken), tagged
/// with the caller's `file:line`.
#[track_caller]
pub fn warn(message: impl Display) {
    write(Level::Warn, Location::caller(), &message.to_string());
}

/// Installs a panic hook that writes the panic message, its location and a
/// backtrace to the log before the default hook prints it. Called once at
/// the start of `main`.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let location = info.location().map_or_else(
            || "unknown".to_string(),
            |l| format!("{}:{}", l.file(), l.line()),
        );
        append(&format!(
            "{} {} {location} {message}\n{backtrace}\n",
            timestamp(),
            Level::Panic.label()
        ));
        default(info);
    }));
}

/// Formats one entry, prints it on stderr and appends it to the file.
fn write(level: Level, location: &Location<'_>, message: &str) {
    let line = format!(
        "{} {} {}:{} {message}",
        timestamp(),
        level.label(),
        location.file(),
        location.line()
    );
    eprintln!("{line}");
    append(&format!("{line}\n"));
}

/// Appends `text` to the log file, rotating it first when too big. Logging
/// must never fail the caller, so I/O errors are dropped.
fn append(text: &str) {
    let Some(path) = path() else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    if let Ok(mut file) = file {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Path of the log file; `None` in snapshots, tests or without a home.
pub fn path() -> Option<PathBuf> {
    crate::settings::user_config_dir().map(|dir| dir.join(FILE))
}

/// Current time as `YYYY-MM-DDTHH:MM:SSZ` (UTC), without a date crate.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

/// Formats seconds since the Unix epoch as an ISO 8601 UTC time, using
/// Howard Hinnant's days-to-civil algorithm.
fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, rem % 3600 / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_utc_should_print_the_epoch() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_utc_should_handle_a_leap_day() {
        assert_eq!(format_utc(1_709_210_096), "2024-02-29T12:34:56Z");
    }
}

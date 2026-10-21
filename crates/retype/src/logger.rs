//! Minimal file logger for the `log` facade.
//!
//! Logs contain event kinds and counts only, never key content (SPEC P4):
//! that is a rule for callers, this module just writes lines.

#![cfg_attr(not(windows), allow(dead_code))]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};
use retype_core::config::LogLevel;

/// A log file larger than this is moved to `<name>.old` at startup.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_line(unix_millis(), record.level(), record.args());
        if let Ok(mut file) = self.file.lock() {
            // Nowhere to report a failed log write; drop the line.
            let _ = file.write_all(line.as_bytes());
        }
        // Debug builds keep a console (main.rs): mirror the log there.
        if cfg!(debug_assertions) {
            let _ = io::stderr().write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Opens (creating directories as needed) and installs the logger.
pub fn init(path: &Path, level: LogLevel) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    rotate(path);
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let logger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
    }));
    log::set_logger(logger).map_err(|_| io::Error::other("a logger is already installed"))?;
    log::set_max_level(level_filter(level));
    Ok(())
}

pub fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
    }
}

fn rotate(path: &Path) {
    let too_big = fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES);
    if too_big {
        let _ = fs::rename(path, path.with_extension("log.old"));
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// `2026-10-21T20:30:00.123Z INFO  message\n` (UTC).
fn format_line(millis: u64, level: log::Level, args: &std::fmt::Arguments<'_>) -> String {
    let secs = millis / 1000;
    let (year, month, day) = civil_from_days(secs / 86_400);
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z {level:<5} {args}\r\n",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        millis % 1000,
    )
}

/// Days since 1970-01-01 to a (year, month, day) date. Howard Hinnant's
/// algorithm, restricted to dates after the epoch.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_747), (2026, 10, 21));
    }

    #[test]
    fn line_format() {
        let millis = 20_747 * 86_400_000 + (20 * 3600 + 30 * 60 + 5) * 1000 + 42;
        let line = format_line(millis, log::Level::Info, &format_args!("started"));
        assert_eq!(line, "2026-10-21T20:30:05.042Z INFO  started\r\n");
    }
}

//! Minimal file logger (the terminal belongs to the UI).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

struct FileLogger {
    file: Mutex<File>,
    level: log::LevelFilter,
}

impl log::Log for FileLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        // librespot is chatty; keep it to warnings unless debugging.
        let lvl = if meta.target().starts_with("librespot") && self.level < log::LevelFilter::Trace {
            self.level.min(log::LevelFilter::Info)
        } else {
            self.level
        };
        meta.level() <= lvl
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Ok(mut f) = self.file.lock() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "{ts} {:<5} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

pub fn init(path: &Path, level: log::LevelFilter) {
    // Keep the log from growing forever.
    if std::fs::metadata(path).map(|m| m.len() > 5_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(path);
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else { return };
    let logger = FileLogger { file: Mutex::new(file), level };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
}

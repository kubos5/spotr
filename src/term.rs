//! Terminal ownership and protection.
//!
//! While the UI runs, the process's stdout/stderr are pointed at the log file
//! and the UI draws to a private duplicate of the terminal. Libraries that
//! print (or audio frameworks that log to stderr) therefore can't corrupt the
//! screen. Panics on background threads are logged and reported in the UI
//! instead of tearing the terminal down.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crossterm::event::{DisableBracketedPaste, DisableMouseCapture};
use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};
use crossterm::{cursor, execute};

/// The real terminal, once stdout has been redirected.
static TTY: OnceLock<File> = OnceLock::new();
/// Original stderr, to restore before printing a fatal panic.
#[cfg(unix)]
static ORIG_ERR: OnceLock<i32> = OnceLock::new();
/// The thread running the UI; a panic there is fatal.
static UI_THREAD: OnceLock<std::thread::ThreadId> = OnceLock::new();

pub fn set_ui_thread() {
    let _ = UI_THREAD.set(std::thread::current().id());
}

fn on_ui_thread() -> bool {
    match UI_THREAD.get() {
        Some(id) => *id == std::thread::current().id(),
        None => std::thread::current().name() == Some("main"),
    }
}

/// Panic messages from background threads, picked up by the UI.
static BACKGROUND_PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Where the UI should write.
pub fn out() -> Box<dyn Write + Send> {
    match TTY.get().and_then(|f| f.try_clone().ok()) {
        Some(f) => Box::new(f),
        None => Box::new(io::stdout()),
    }
}

/// Point stdout/stderr at `log` and keep a private handle to the terminal.
#[cfg(unix)]
pub fn redirect_output(log: &Path) {
    use std::os::fd::{AsRawFd, FromRawFd};
    let _ = io::stdout().flush();
    let Ok(logf) = std::fs::OpenOptions::new().create(true).append(true).open(log) else { return };
    // SAFETY: plain fd duplication; the new fds are owned by the Files we create.
    unsafe {
        let tty = libc::dup(1);
        let err = libc::dup(2);
        if tty < 0 || err < 0 {
            return;
        }
        let _ = TTY.set(File::from_raw_fd(tty));
        let _ = ORIG_ERR.set(err);
        libc::dup2(logf.as_raw_fd(), 1);
        libc::dup2(logf.as_raw_fd(), 2);
        // If a library calls exit(), still give the user their terminal back.
        libc::atexit(at_exit);
    }
}

#[cfg(not(unix))]
pub fn redirect_output(_log: &Path) {}

#[cfg(unix)]
extern "C" fn at_exit() {
    restore();
}

/// Put fds back so messages after the UI reach the user's terminal.
fn restore_fds() {
    #[cfg(unix)]
    // SAFETY: restoring fds we duplicated in `redirect_output`.
    unsafe {
        use std::os::fd::AsRawFd;
        if let Some(t) = TTY.get() {
            libc::dup2(t.as_raw_fd(), 1);
        }
        if let Some(e) = ORIG_ERR.get() {
            libc::dup2(*e, 2);
        }
    }
}

/// Leave raw mode / the alternate screen. Safe to call more than once.
pub fn restore() {
    let mut o = out();
    let _ = execute!(o, DisableMouseCapture, DisableBracketedPaste, LeaveAlternateScreen, cursor::Show);
    let _ = disable_raw_mode();
    let _ = o.flush();
    restore_fds();
}

/// Install the panic hook. Only a panic on the UI (main) thread is fatal.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let main = on_ui_thread();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        log::error!("panic on thread {:?} at {loc}: {msg}", std::thread::current().name());
        if main {
            restore();
            default_hook(info);
        } else if let Ok(mut v) = BACKGROUND_PANICS.lock() {
            // Keep the first line only; the full message is in the log.
            v.push(msg.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string());
        }
    }));
}

pub fn take_background_panics() -> Vec<String> {
    BACKGROUND_PANICS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

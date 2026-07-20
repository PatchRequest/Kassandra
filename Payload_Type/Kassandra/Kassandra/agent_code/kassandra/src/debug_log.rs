//! File-based debug logging for lab diagnostics.
//!
//! Writes to `%TEMP%\kassandra_debug.log` (or `C:\Windows\Temp` fallback).
//! Always enabled for now — this agent is not production-hardened for silent
//! operation in this tree. Strip or gate later if needed.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

fn log_path() -> PathBuf {
    let mut guard = LOG_PATH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref p) = *guard {
        return p.clone();
    }
    let dir = std::env::temp_dir();
    let p = dir.join("kassandra_debug.log");
    *guard = Some(p.clone());
    p
}

/// Absolute path of the active debug log file.
pub fn path() -> PathBuf {
    log_path()
}

fn timestamp() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", dur.as_secs(), dur.subsec_millis())
}

/// Append one log line. Never panics; failures are silent.
pub fn log(msg: &str) {
    let path = log_path();
    let line = format!("[{}] {}\r\n", timestamp(), msg);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
    // Also mirror to stderr when a console is attached
    let _ = writeln!(std::io::stderr(), "[kassandra] {}", msg);
}

/// `log` with `format!` args.
#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {{
        $crate::debug_log::log(&format!($($arg)*));
    }};
}

/// Install a panic hook that records panics to the debug log before aborting.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".into()
        };
        log(&format!("PANIC at {loc}: {msg}"));
        default(info);
    }));
}

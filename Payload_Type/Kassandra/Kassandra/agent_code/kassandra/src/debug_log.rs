//! Optional diagnostic logging.
//!
//! Enabled only with the `debug_log` cargo feature (builder parameter).
//! Production builds compile every `dlog!` call to a no-op — no file paths,
//! no stderr mirrors, no format-string evaluation.

#[cfg(feature = "debug_log")]
mod active {
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
        let p = std::env::temp_dir().join("kassandra_debug.log");
        *guard = Some(p.clone());
        p
    }

    pub fn path() -> PathBuf {
        log_path()
    }

    fn timestamp() -> String {
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}.{:03}", dur.as_secs(), dur.subsec_millis())
    }

    pub fn log(msg: &str) {
        let path = log_path();
        let line = format!("[{}] {}\r\n", timestamp(), msg);
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }

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
}

#[cfg(feature = "debug_log")]
pub use active::{install_panic_hook, log, path};

#[cfg(not(feature = "debug_log"))]
pub fn install_panic_hook() {}

#[cfg(not(feature = "debug_log"))]
pub fn path() -> std::path::PathBuf {
    std::path::PathBuf::new()
}

/// Log a diagnostic line when `debug_log` is enabled; no-op otherwise.
#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {{
        #[cfg(feature = "debug_log")]
        {
            $crate::debug_log::log(&format!($($arg)*));
        }
    }};
}

